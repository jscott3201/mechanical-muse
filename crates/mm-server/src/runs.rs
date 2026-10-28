//! Run sessions. One live run at a time (BOPTEST has one worker) and, beside it, one replay at a
//! time (a replay drives no plant, so a live run stuck selecting or initializing must not block
//! it). Each run is one task that owns its
//! engines and its plant; it publishes events to a log (for late subscribers and reconnects), a
//! broadcast channel (for live subscribers) and, if configured, a JSON-lines record on disk.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use mm_runtime::{FakePlant, Measurements};
use tokio::sync::watch;

use crate::journal::{Entry, Journal, lock};
use crate::run_dto::{Building, Envelope, RunConfig, RunEvent, RunInfo, RunRequest, RunState};

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
            RunEvent::PeakPower { .. } => "peak-power",
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
    /// No plant: live runs are refused, replays still work. The server's default when
    /// `MM_BOPTEST_URL` is unset, so a Start click never reaches whatever listens on port 80.
    Off,
}

impl PlantSource {
    /// True when a live run has a plant to drive.
    pub fn is_live(&self) -> bool {
        !matches!(self, PlantSource::Off)
    }
}

#[derive(Debug, Clone)]
pub struct RunsConfig {
    pub plant: PlantSource,
    /// Directory for JSON-lines run records; `None` keeps runs in memory only.
    pub records: Option<PathBuf>,
    /// Directory of recorded runs that may be replayed (`<name>.jsonl`); `None` disables replay.
    pub replays: Option<PathBuf>,
    /// Wall time between replayed ticks.
    pub replay_pace: std::time::Duration,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StartError {
    #[error("a run is already active: {0}")]
    Active(String),
    #[error("{0}")]
    Invalid(String),
    /// A recorded authoring session was asked for and none exists for that spec.
    #[error("{0}")]
    NoRecording(String),
}

pub struct RunHandle {
    pub id: String,
    pub config: RunConfig,
    state: Mutex<RunState>,
    journal: Arc<Journal<Envelope>>,
    stop: watch::Sender<bool>,
    ticks: AtomicU32,
    kpis: Mutex<Option<BTreeMap<String, f64>>>,
    peak: Mutex<Option<crate::home_facts::PeakPower>>,
    /// The BOPTEST test this run selected, once it has one (for logs at exit).
    testid: Mutex<Option<String>>,
}

impl RunHandle {
    pub fn info(&self) -> RunInfo {
        RunInfo {
            id: self.id.clone(),
            config: self.config.clone(),
            state: *lock(&self.state),
            ticks: self.ticks.load(Ordering::Relaxed),
            events: u32::try_from(self.journal.len()).unwrap_or(u32::MAX),
            kpis: lock(&self.kpis).clone(),
            peak: lock(&self.peak).clone(),
        }
    }

    pub fn state(&self) -> RunState {
        *lock(&self.state)
    }

    pub(crate) fn set_state(&self, state: RunState) {
        *lock(&self.state) = state;
    }

    pub(crate) fn set_testid(&self, testid: &str) {
        *lock(&self.testid) = Some(testid.to_owned());
    }

    /// The BOPTEST test this run selected, if it got that far.
    pub fn testid(&self) -> Option<String> {
        lock(&self.testid).clone()
    }

    /// True for a replay of a recorded run (no plant).
    pub fn is_replay(&self) -> bool {
        self.config.replay.is_some()
    }

    pub(crate) fn count_tick(&self) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// Appends an event to the log, the record and the live channel, in that order.
    pub(crate) fn publish(&self, sim_time_s: Option<f64>, event: RunEvent) {
        match &event {
            RunEvent::Kpi { values } => *lock(&self.kpis) = Some(values.clone()),
            RunEvent::PeakPower { peak } => *lock(&self.peak) = Some(peak.clone()),
            _ => {}
        }
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

    /// The live (BOPTEST) run that is starting or running, if any. Replays are not counted.
    pub fn active(&self) -> Option<Arc<RunHandle>> {
        lock(&self.registry)
            .runs
            .values()
            .find(|r| !r.is_replay() && is_active(r))
            .cloned()
    }

    /// Every run that is starting or running, live or replay.
    pub fn all_active(&self) -> Vec<Arc<RunHandle>> {
        lock(&self.registry)
            .runs
            .values()
            .filter(|r| is_active(r))
            .cloned()
            .collect()
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

    /// Registers a new live run if no other live run is active. The caller spawns its task.
    pub fn create(&self, request: &RunRequest) -> Result<Arc<RunHandle>, StartError> {
        self.register(resolve(request)?)
    }

    /// Registers a run with a settled configuration (a live run, or a replay). A live run waits
    /// for no replay and a replay for no live run: each kind is refused only while another run
    /// of its own kind is active.
    pub(crate) fn register(&self, config: RunConfig) -> Result<Arc<RunHandle>, StartError> {
        let mut reg = lock(&self.registry);
        let replay = config.replay.is_some();
        if let Some(active) = reg
            .runs
            .values()
            .find(|r| r.is_replay() == replay && is_active(r))
        {
            return Err(StartError::Active(active.id.clone()));
        }
        let id = crate::journal::fresh_id("run", &mut reg.next, self.config.records.as_deref());
        let journal = Arc::new(Journal::new(&id, self.config.records.as_deref()));
        let (stop, _) = watch::channel(false);
        let handle = Arc::new(RunHandle {
            id: id.clone(),
            config,
            state: Mutex::new(RunState::Starting),
            journal,
            stop,
            ticks: AtomicU32::new(0),
            kpis: Mutex::new(None),
            peak: Mutex::new(None),
            testid: Mutex::new(None),
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
            PlantSource::Boptest { .. } | PlantSource::Off => None,
        }
    }
}

fn is_active(run: &RunHandle) -> bool {
    matches!(run.state(), RunState::Starting | RunState::Running)
}

pub(crate) fn resolve(r: &RunRequest) -> Result<RunConfig, StartError> {
    let home = r.building.unwrap_or_default() == Building::Home;
    if home {
        check_home(r)?;
    }
    // The home's defaults: BOPTEST's typical heat window (from day 108), a week of warm-up,
    // two weeks of 15-minute steps (the length the home's results were measured over).
    let start_s = r.start_s.unwrap_or(if home {
        108.0 * 86_400.0
    } else {
        185.0 * 86_400.0 + 12.0 * 3_600.0
    });
    let warmup_s = r
        .warmup_s
        .unwrap_or(if home { MAX_WARMUP_S } else { 86_400.0 });
    let steps = r.steps.unwrap_or(if home { 1344 } else { 300 });
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
    if let Some(f) = &r.fault {
        if r.baseline.unwrap_or(false) {
            return Err(StartError::Invalid(
                "a baseline run has no sequences to fault".into(),
            ));
        }
        if !fault_points().any(|p| p == f.point) {
            return Err(StartError::Invalid(format!(
                "{} is not a reading the sequences use",
                f.point
            )));
        }
        if f.from_step >= steps {
            return Err(StartError::Invalid(format!(
                "the fault starts at step {} but the run has {steps}",
                f.from_step
            )));
        }
        if f.steps == 0 || f.steps > 60 {
            return Err(StartError::Invalid("a fault lasts 1 to 60 steps".into()));
        }
        // Without the request sequences the reset reads only occupancy: a fault elsewhere would
        // be logged as injected and change nothing.
        if !r.requests.unwrap_or(true) && !f.point.ends_with("_reaAHU_occ_y") {
            return Err(StartError::Invalid(format!(
                "without air handler requests only occupancy readings are used, not {}",
                f.point
            )));
        }
        if f.mode.amount().is_some_and(|v| !v.is_finite()) {
            return Err(StartError::Invalid(
                "a held value or bias must be a finite number".into(),
            ));
        }
    }
    let baseline = r.baseline.unwrap_or(false);
    if baseline && r.drafts.as_ref().is_some_and(|d| !d.is_empty()) {
        return Err(StartError::Invalid(
            "a baseline run uses no sequences, so it takes no drafts".into(),
        ));
    }
    Ok(RunConfig {
        baseline,
        // A baseline runs no sequence, so it uses no requests either; the home has none.
        requests: !home && !baseline && r.requests.unwrap_or(true),
        pressure: !home && !baseline && r.pressure.unwrap_or(false),
        start_s,
        warmup_s,
        steps,
        step_s: if home {
            mm_runtime::home::STEP_S
        } else {
            mm_runtime::preset::STEP_S
        },
        case: if home {
            mm_runtime::home::CASE
        } else {
            mm_runtime::preset::CASE
        }
        .into(),
        drafts: r.drafts.clone().unwrap_or_default(),
        fault: r.fault.clone(),
        bindings: BTreeMap::new(),
        replay: None,
    })
}

/// The home runs one sequence on its own building: the office's options do not apply.
fn check_home(r: &RunRequest) -> Result<(), StartError> {
    if r.requests == Some(true) || r.pressure == Some(true) {
        return Err(StartError::Invalid(
            "the home has no air handlers: requests and pressure are office options".into(),
        ));
    }
    if r.fault.is_some() {
        return Err(StartError::Invalid(
            "test faults are defined for the office's air handler readings only".into(),
        ));
    }
    if let Some(drafts) = &r.drafts
        && let Some(other) = drafts.keys().find(|k| *k != mm_runtime::home::SPEC)
    {
        return Err(StartError::Invalid(format!(
            "the home runs {} only, not {other}",
            mm_runtime::home::SPEC
        )));
    }
    Ok(())
}

/// The readings the demo sequences use, which a test fault may replace.
pub fn fault_points() -> impl Iterator<Item = String> {
    mm_runtime::preset::FLOORS.iter().flat_map(|f| {
        [
            "reaAHU_TSup_y",
            "TSupAirSet_u",
            "reaAHU_yCooVal_y",
            "reaAHU_occ_y",
        ]
        .map(move |p| format!("floor{f}_{p}"))
    })
}
