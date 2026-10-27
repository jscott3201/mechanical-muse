//! The task behind one run: verify the specs, load the demo wiring, drive the plant, publish
//! every tick. A live BOPTEST test is always stopped at the end, whatever happened, so the
//! single worker is freed.

use std::collections::BTreeMap;
use std::sync::Arc;

use mm_runtime::preset::{self, FLOORS, REQUESTS_SPEC, RESET, RESET_SPEC};
use mm_runtime::{
    BoptestClient, Event, FailSafe, FakePlant, Measurements, Plant, PlantError, RunningSequence,
    Supervisor, TickReport, Writes,
};

use crate::routes::AppState;
use crate::run_dto::{Phase, RunConfig, RunEvent, RunState, SequenceInfo, SupervisorEvent};
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
    let (final_state, message) = match drive(&handle, &state).await {
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

/// Loads `spec`'s reference graph only if its vectors replay green (AGENTS.md hard rule 5).
async fn load(
    state: &AppState,
    spec: &str,
    instances: Vec<(String, Vec<mm_runtime::InputBinding>)>,
    out: &mut Vec<SequenceInfo>,
) -> Result<Vec<RunningSequence>, String> {
    let found = state
        .specs()
        .get(spec)
        .ok_or_else(|| format!("spec {spec} is not loaded"))?
        .clone();
    let worker = state.clone();
    let (cxf, report) = tokio::task::spawn_blocking(move || found.replay(worker.palette()))
        .await
        .map_err(|e| format!("verifying {spec} stopped: {e}"))??;
    if !report.passed() {
        return Err(format!(
            "{spec} did not pass its vectors; refusing to run it"
        ));
    }
    let mut loaded = Vec::new();
    for (name, bindings) in instances {
        out.push(SequenceInfo {
            name: name.clone(),
            spec: spec.to_owned(),
            content_id: report.content_id.clone(),
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
        sequences.extend(load(state, REQUESTS_SPEC, instances, &mut info).await?);
    }
    let reset = vec![(RESET.to_owned(), preset::reset_bindings(config.requests))];
    sequences.extend(load(state, RESET_SPEC, reset, &mut info).await?);
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
            let client = BoptestClient::select(url, preset::CASE)
                .await
                .map_err(|e| format!("selecting the test case: {e}"))?;
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
                    stop_quietly(&client).await;
                    return Err(format!("initializing BOPTEST: {e}"));
                }
            }
        }
    };
    let outcome = steps(handle, &mut sup, &mut plant, first).await;
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
    for _ in 0..handle.config.steps {
        // A stop is honoured between steps: an advance already sent is one state transition
        // of the plant and is never abandoned halfway.
        if *stop.borrow() {
            return Ok(RunState::Stopped);
        }
        let report = sup.tick(&readings);
        handle.publish(Some(report.time), tick(&report, &readings, &watch));
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
            value: value.is_finite().then_some(*value),
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
