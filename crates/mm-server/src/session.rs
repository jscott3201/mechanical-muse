//! The task behind one run: verify the specs, load the demo wiring, drive the plant, publish
//! every tick. A live BOPTEST test is stopped at the end whatever happened (a panic in the
//! loop included), so the single worker is freed, and every run ends with an `ended` event.
//!
//! A stop request is honoured before `select` and between steps. It is deferred while BOPTEST
//! initializes (minutes under emulation): stopping a test that is still starting crashed the
//! v0.9.0 web service, so the run waits for the warm-up to finish, then stops.

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt;

use mm_runtime::preset::{self, FLOORS, REQUESTS_SPEC, RESET, RESET_SPEC};
use mm_runtime::{
    BoptestClient, Event, FailSafe, FakePlant, Measurements, Plant, PlantError, RunningSequence,
    Supervisor, TickReport, Writes,
};

use crate::routes::AppState;
use crate::run_dto::{Origin, Phase, RunConfig, RunEvent, RunState, SequenceInfo, SupervisorEvent};
use crate::runs::{PlantSource, RunHandle};

enum AnyPlant {
    Boptest(BoptestClient),
    Fake(FakePlant, std::time::Duration),
}

impl Plant for AnyPlant {
    async fn advance(&mut self, writes: &Writes) -> Result<Measurements, PlantError> {
        match self {
            AnyPlant::Boptest(p) => p.advance(writes).await,
            AnyPlant::Fake(p, pace) => {
                tokio::time::sleep(*pace).await;
                p.advance(writes).await
            }
        }
    }
}

/// Runs to the end and publishes the final `ended` event.
pub async fn run(handle: Arc<RunHandle>, state: AppState) {
    let outcome = AssertUnwindSafe(drive(&handle, &state))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err("the run task panicked".into()));
    let (final_state, message) = match outcome {
        Ok(s) => (s, None),
        Err(message) => {
            tracing::warn!(run = %handle.id, %message, "run failed");
            (RunState::Failed, Some(message))
        }
    };
    handle.set_state(final_state);
    handle.publish(
        None,
        RunEvent::Ended {
            state: final_state,
            message,
        },
    );
}

/// Loads `spec`'s graph only if its vectors replay green now (AGENTS.md hard rule 5): the
/// reference graph, or the verified draft the run asked for, whose fresh content id must also
/// match the one it was stored under.
async fn load(
    state: &AppState,
    config: &RunConfig,
    spec: &str,
    instances: Vec<(String, Vec<mm_runtime::InputBinding>)>,
    out: &mut Vec<SequenceInfo>,
) -> Result<Vec<RunningSequence>, String> {
    let found = state
        .specs()
        .get(spec)
        .ok_or_else(|| format!("spec {spec} is not loaded"))?
        .clone();
    let draft = match config.drafts.get(spec) {
        Some(id) => Some(
            state
                .authoring()
                .drafts
                .get(id)
                .filter(|d| d.info.spec == spec)
                .ok_or_else(|| format!("no verified draft {id} for {spec}"))?,
        ),
        None => None,
    };
    if let Some(d) = &draft {
        // The same static checks authoring ran, in case the stored file changed since.
        let problems = mm_agent::interface_problems(&d.graph, &found.task);
        if let Some(p) = problems.first() {
            return Err(format!("the draft for {spec} no longer fits the task: {p}"));
        }
        if let Some(f) = mm_graph::validate(&d.graph, state.palette()).first() {
            return Err(format!(
                "the draft for {spec} fails validation: {} at {}",
                f.code.as_str(),
                f.subject
            ));
        }
    }
    let worker = state.clone();
    let graph = draft.as_ref().map(|d| d.graph.clone());
    let (cxf, report) = tokio::task::spawn_blocking(move || match &graph {
        Some(g) => found.replay_graph(g, worker.palette()),
        None => found.replay(worker.palette()),
    })
    .await
    .map_err(|e| format!("verifying {spec} stopped: {e}"))??;
    if !report.passed() {
        return Err(format!(
            "{spec} did not pass its vectors; refusing to run it"
        ));
    }
    if let Some(d) = &draft
        && d.info.content_id != report.content_id
    {
        return Err(format!(
            "the draft for {spec} now has content id {}, not {}; refusing to run it",
            report.content_id, d.info.content_id
        ));
    }
    let mut loaded = Vec::new();
    for (name, bindings) in instances {
        out.push(SequenceInfo {
            name: name.clone(),
            spec: spec.to_owned(),
            content_id: report.content_id.clone(),
            origin: if draft.is_some() {
                Origin::Drafted
            } else {
                Origin::Reference
            },
            model: draft.as_ref().map(|d| d.info.model.clone()),
            source: draft.as_ref().map(|d| d.info.source),
        });
        loaded.push(
            RunningSequence::load(&name, &cxf, &report, bindings)
                .map_err(|e| format!("loading {name}: {e}"))?,
        );
    }
    Ok(loaded)
}

async fn supervisor(
    state: &AppState,
    config: &RunConfig,
) -> Result<(Supervisor, Vec<SequenceInfo>), String> {
    let mut info = Vec::new();
    if config.baseline {
        let sup = Supervisor::new(Vec::new(), Vec::new(), FailSafe::Release)
            .map_err(|e| e.to_string())?;
        return Ok((sup, info));
    }
    let mut sequences = Vec::new();
    if config.requests {
        let instances = FLOORS
            .iter()
            .map(|f| (preset::requests_name(*f), preset::request_bindings(*f)))
            .collect();
        sequences.extend(load(state, config, REQUESTS_SPEC, instances, &mut info).await?);
    }
    let reset = vec![(RESET.to_owned(), preset::reset_bindings(config.requests))];
    sequences.extend(load(state, config, RESET_SPEC, reset, &mut info).await?);
    let sup = Supervisor::new(sequences, preset::reset_outputs(), FailSafe::Release)
        .map_err(|e| e.to_string())?;
    Ok((sup, info))
}

async fn drive(handle: &RunHandle, state: &AppState) -> Result<RunState, String> {
    let config = handle.config.clone();
    handle.publish(
        None,
        RunEvent::Phase {
            phase: Phase::Verifying,
        },
    );
    let (mut sup, sequences) = supervisor(state, &config).await?;
    handle.publish(
        None,
        RunEvent::Started {
            config: config.clone(),
            sequences,
        },
    );
    if *handle.stop_signal().borrow() {
        return Ok(RunState::Stopped);
    }
    let (mut plant, first) = match state.runs().fake_plant() {
        Some((fake, first, pace)) => (AnyPlant::Fake(fake, pace), first),
        None => {
            let PlantSource::Boptest { url } = &state.runs().config.plant else {
                return Err("scripted plant has no readings".into());
            };
            handle.publish(
                None,
                RunEvent::Phase {
                    phase: Phase::Selecting,
                },
            );
            // A fresh deployment may still be uploading the test case: wait up to a minute.
            let client = BoptestClient::select_when_ready(
                url,
                preset::CASE,
                7,
                std::time::Duration::from_secs(10),
            )
            .await
            .map_err(|e| {
                // An abandoned select can still create a test, which then holds the worker.
                tracing::error!(error = %e, "select failed; BOPTEST may hold an orphaned test");
                format!("selecting the test case: {e}")
            })?;
            handle.publish(
                None,
                RunEvent::Phase {
                    phase: Phase::Initializing,
                },
            );
            let first = async {
                let first = client.initialize(config.start_s, config.warmup_s).await?;
                client.set_step(config.step_s).await?;
                Ok::<_, PlantError>(first)
            }
            .await;
            match first {
                Ok(first) => (AnyPlant::Boptest(client), first),
                Err(e) => {
                    // After a timeout or a server error the worker may still be initializing,
                    // and stopping it then crashes v0.9.0's web service: leave it to BOPTEST's
                    // idle timeout and say which test it was.
                    let still_starting = matches!(
                        e,
                        PlantError::Transport { .. } | PlantError::Refused { status: 500.., .. }
                    );
                    if still_starting {
                        tracing::error!(testid = client.testid(), error = %e,
                            "initialize failed; not stopping a test that may still be starting");
                    } else {
                        stop_quietly(&client).await;
                    }
                    return Err(format!("initializing BOPTEST: {e}"));
                }
            }
        }
    };
    let bacnet = state.bacnet().map(AsRef::as_ref);
    let outcome = AssertUnwindSafe(steps(handle, &mut sup, &mut plant, first, bacnet))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err("the control loop panicked".into()));
    if let Some(bacnet) = bacnet {
        // Nothing produces values once the run ends, whatever its outcome.
        crate::bacnet::clear(bacnet).await;
    }
    if let AnyPlant::Boptest(client) = &plant {
        match client.kpi().await {
            Ok(values) => handle.publish(None, RunEvent::Kpi { values }),
            Err(e) => tracing::warn!(run = %handle.id, error = %e, "no KPIs"),
        }
        stop_quietly(client).await;
    }
    outcome
}

async fn stop_quietly(client: &BoptestClient) {
    if let Err(e) = client.stop().await {
        tracing::warn!(error = %e, "stopping the BOPTEST test failed");
    }
}

async fn steps(
    handle: &RunHandle,
    sup: &mut Supervisor,
    plant: &mut AnyPlant,
    mut readings: Measurements,
    bacnet: Option<&crate::bacnet::BacnetMirror>,
) -> Result<RunState, String> {
    let stop = handle.stop_signal();
    let watch = preset::watch_points();
    handle.set_state(RunState::Running);
    handle.publish(
        None,
        RunEvent::Phase {
            phase: Phase::Running,
        },
    );
    for step in 0..handle.config.steps {
        // A stop is honoured between steps: an advance already sent is one state transition
        // of the plant and is never abandoned halfway.
        if *stop.borrow() {
            return Ok(RunState::Stopped);
        }
        let fault = handle
            .config
            .fault
            .as_ref()
            .filter(|f| (f.from_step..f.from_step.saturating_add(f.steps)).contains(&step));
        let seen = match fault {
            Some(f) => {
                let mut faulty = readings.clone();
                faulty.insert(f.point.clone(), f64::NAN);
                faulty
            }
            None => readings.clone(),
        };
        let report = sup.tick(&seen);
        let time = report.time.is_finite().then_some(report.time);
        let mut event = tick(&report, &seen, &watch);
        if let (Some(f), RunEvent::Tick { events, .. }) = (fault, &mut event) {
            events.insert(
                0,
                SupervisorEvent {
                    kind: "injected".into(),
                    sequence: None,
                    point: Some(f.point.clone()),
                    value: None,
                    sent: None,
                    reason: Some("test fault: reading replaced with NaN".into()),
                },
            );
        }
        if let Some(bacnet) = bacnet {
            crate::bacnet::update(bacnet, &event).await;
        }
        handle.publish(time, event);
        handle.count_tick();
        readings = match plant.advance(&report.writes).await {
            Ok(next) => next,
            Err(PlantError::Ended) => break,
            Err(e) => return Err(format!("advancing the plant: {e}")),
        };
    }
    Ok(RunState::Completed)
}

fn tick(report: &TickReport, readings: &Measurements, watch: &[String]) -> RunEvent {
    let readings: BTreeMap<String, f64> = watch
        .iter()
        .filter_map(|p| Some((p.clone(), *readings.get(p)?)))
        .filter(|(_, v)| v.is_finite())
        .collect();
    RunEvent::Tick {
        outputs: serde_json::to_value(&report.outputs).unwrap_or(serde_json::Value::Null),
        writes: report.writes.clone(),
        readings,
        events: report.events.iter().map(flatten).collect(),
    }
}

fn flatten(event: &Event) -> SupervisorEvent {
    let base = |kind: &str| SupervisorEvent {
        kind: kind.to_owned(),
        sequence: None,
        point: None,
        value: None,
        sent: None,
        reason: None,
    };
    match event {
        Event::Stale { .. } => base("stale"),
        Event::Degraded { sequence, reason } => SupervisorEvent {
            sequence: Some(sequence.clone()),
            reason: Some(reason.to_string()),
            ..base("degraded")
        },
        Event::Clamped { point, value, sent } => SupervisorEvent {
            point: Some(point.clone()),
            value: Some(*value),
            sent: Some(*sent),
            ..base("clamped")
        },
        Event::Rejected { point } => SupervisorEvent {
            point: Some(point.clone()),
            ..base("rejected")
        },
        Event::Held { point, value } => SupervisorEvent {
            point: Some(point.clone()),
            value: Some(*value),
            ..base("held")
        },
        Event::Released { point } => SupervisorEvent {
            point: Some(point.clone()),
            ..base("released")
        },
        Event::NoSuchOutput { sequence, output } => SupervisorEvent {
            sequence: Some(sequence.clone()),
            point: Some(output.clone()),
            ..base("no-such-output")
        },
    }
}
