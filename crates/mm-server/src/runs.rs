//! Run sessions. One run at a time (BOPTEST has one worker). Each run is one task that owns its
//! engines and its plant; it publishes events to a log (for late subscribers and reconnects), a
//! broadcast channel (for live subscribers) and, if configured, a JSON-lines record on disk.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use mm_runtime::{FakePlant, Measurements};
use tokio::sync::watch;

use crate::journal::{Entry, Journal, lock};
use crate::run_dto::{Envelope, RunConfig, RunEvent, RunInfo, RunRequest, RunState};

/// Largest run a client may ask for (about a week of 120 s steps).
pub const MAX_STEPS: u32 = 5_040;
const MAX_WARMUP_S: f64 = 7.0 * 86_400.0;
const YEAR_S: f64 = 365.0 * 86_400.0;

impl Entry for Envelope {
    fn seq(&self) -> u64 {
        self.seq
    }

    fn kind(&self) -> &'static str {
        match &self.event {
            RunEvent::Started { .. } => "started",
            RunEvent::Phase { .. } => "phase",
            RunEvent::Tick { .. } => "tick",
            RunEvent::Kpi { .. } => "kpi",
            RunEvent::Ended { .. } => "ended",
        }
    }

    fn is_last(&self) -> bool {
        matches!(self.event, RunEvent::Ended { .. })
    }
}

/// Where a run's plant comes from.
#[derive(Debug, Clone)]
pub enum PlantSource {
    Boptest {
        url: String,
    },
    /// Scripted readings (tests, demo replays): the first is the initial state, then one per
    /// advance, each after `pace` of wall time.
    Scripted {
        readings: Vec<Measurements>,
        pace: std::time::Duration,
    },
}

#[derive(Debug, Clone)]
pub struct RunsConfig {
    pub plant: PlantSource,
    /// Directory for JSON-lines run records; `None` keeps runs in memory only.
    pub records: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StartError {
    #[error("a run is already active: {0}")]
    Active(String),
    #[error("{0}")]
    Invalid(String),
}

pub struct RunHandle {
    pub id: String,
    pub config: RunConfig,
    state: Mutex<RunState>,
    journal: Arc<Journal<Envelope>>,
    stop: watch::Sender<bool>,
    ticks: AtomicU32,
}

impl RunHandle {
    pub fn info(&self) -> RunInfo {
        RunInfo {
            id: self.id.clone(),
            config: self.config.clone(),
            state: *lock(&self.state),
            ticks: self.ticks.load(Ordering::Relaxed),
            events: u32::try_from(self.journal.len()).unwrap_or(u32::MAX),
        }
    }

    pub fn state(&self) -> RunState {
        *lock(&self.state)
    }

    pub(crate) fn set_state(&self, state: RunState) {
        *lock(&self.state) = state;
    }

    pub(crate) fn count_tick(&self) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// Appends an event to the log, the record and the live channel, in that order.
    pub(crate) fn publish(&self, sim_time_s: Option<f64>, event: RunEvent) {
        self.journal.append(|seq| Envelope {
            run_id: self.id.clone(),
            seq,
            sim_time_s,
            event,
        });
    }

    pub fn journal(&self) -> Arc<Journal<Envelope>> {
        Arc::clone(&self.journal)
    }

    pub fn request_stop(&self) {
        // send() is a no-op while nobody holds a receiver (the run task only subscribes at
        // certain points, e.g. not while verifying); send_replace always stores the value.
        self.stop.send_replace(true);
    }

    pub(crate) fn stop_signal(&self) -> watch::Receiver<bool> {
        self.stop.subscribe()
    }
}

#[derive(Default)]
struct Registry {
    next: u64,
    runs: BTreeMap<String, Arc<RunHandle>>,
}

pub struct Runs {
    pub config: RunsConfig,
    registry: Mutex<Registry>,
}

impl Runs {
    pub fn new(config: RunsConfig) -> Self {
        Self {
            config,
            registry: Mutex::new(Registry::default()),
        }
    }

    /// The run that is starting or running, if any.
    pub fn active(&self) -> Option<Arc<RunHandle>> {
        lock(&self.registry)
            .runs
            .values()
            .find(|r| matches!(r.state(), RunState::Starting | RunState::Running))
            .cloned()
    }

    pub fn get(&self, id: &str) -> Option<Arc<RunHandle>> {
        lock(&self.registry).runs.get(id).cloned()
    }

    pub fn list(&self) -> Vec<RunInfo> {
        lock(&self.registry)
            .runs
            .values()
            .map(|r| r.info())
            .collect()
    }

    /// Registers a new run if none is active. The caller spawns its task.
    pub fn create(&self, request: &RunRequest) -> Result<Arc<RunHandle>, StartError> {
        let config = resolve(request)?;
        let mut reg = lock(&self.registry);
        if let Some(active) = reg
            .runs
            .values()
            .find(|r| matches!(r.state(), RunState::Starting | RunState::Running))
        {
            return Err(StartError::Active(active.id.clone()));
        }
        reg.next += 1;
        // Ids sort in start order; the prefix keeps them unique across server restarts.
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let id = format!("run-{started}-{}", reg.next);
        let journal = Arc::new(Journal::new(&id, self.config.records.as_deref()));
        let (stop, _) = watch::channel(false);
        let handle = Arc::new(RunHandle {
            id: id.clone(),
            config,
            state: Mutex::new(RunState::Starting),
            journal,
            stop,
            ticks: AtomicU32::new(0),
        });
        reg.runs.insert(id, Arc::clone(&handle));
        Ok(handle)
    }

    pub(crate) fn fake_plant(&self) -> Option<(FakePlant, Measurements, std::time::Duration)> {
        match &self.config.plant {
            PlantSource::Scripted { readings, pace } => {
                let mut script = readings.clone();
                if script.is_empty() {
                    return None;
                }
                let first = script.remove(0);
                Some((FakePlant::new(script), first, *pace))
            }
            PlantSource::Boptest { .. } => None,
        }
    }
}

fn resolve(r: &RunRequest) -> Result<RunConfig, StartError> {
    let start_s = r.start_s.unwrap_or(185.0 * 86_400.0 + 12.0 * 3_600.0);
    let warmup_s = r.warmup_s.unwrap_or(86_400.0);
    let steps = r.steps.unwrap_or(300);
    if !(start_s.is_finite() && (0.0..YEAR_S).contains(&start_s)) {
        return Err(StartError::Invalid(
            "start_s must be within the year".into(),
        ));
    }
    if !(warmup_s.is_finite() && (0.0..=MAX_WARMUP_S).contains(&warmup_s)) {
        return Err(StartError::Invalid("warmup_s must be 0 to 7 days".into()));
    }
    if start_s < warmup_s {
        return Err(StartError::Invalid(
            "start_s must be at least warmup_s (the warm-up runs before the start)".into(),
        ));
    }
    if !(1..=MAX_STEPS).contains(&steps) {
        return Err(StartError::Invalid(format!(
            "steps must be 1 to {MAX_STEPS}"
        )));
    }
    let baseline = r.baseline.unwrap_or(false);
    if baseline && r.drafts.as_ref().is_some_and(|d| !d.is_empty()) {
        return Err(StartError::Invalid(
            "a baseline run uses no sequences, so it takes no drafts".into(),
        ));
    }
    Ok(RunConfig {
        baseline,
        // A baseline runs no sequence, so it uses no requests either.
        requests: !baseline && r.requests.unwrap_or(true),
        start_s,
        warmup_s,
        steps,
        step_s: mm_runtime::preset::STEP_S,
        case: mm_runtime::preset::CASE.into(),
        drafts: r.drafts.clone().unwrap_or_default(),
    })
}
