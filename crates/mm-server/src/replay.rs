//! Replaying a recorded run (the demo's "recorded run" fallback). The recorded events are sent
//! again in order through a normal run's journal and event stream, one tick per `pace`, with
//! fresh sequence numbers and a config that names the record, so every view can say "replay".
//! Nothing runs against a plant and no sequence is loaded.

use std::path::Path;
use std::sync::Arc;

use crate::run_dto::{Envelope, ReplayInfo, ReplaySource, RunConfig, RunEvent, RunState};
use crate::runs::{RunHandle, StartError};

/// A record name is a file stem: letters, digits, `-`, `_` and `.`, nothing that walks paths.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Every event of `<dir>/<name>.jsonl`, which must hold a `started` event.
pub fn read(dir: &Path, name: &str) -> Result<Vec<Envelope>, String> {
    if !valid_name(name) {
        return Err(format!("{name:?} is not a record name"));
    }
    let text = std::fs::read_to_string(dir.join(format!("{name}.jsonl")))
        .map_err(|_| format!("no recorded run named {name}"))?;
    let mut events = Vec::new();
    for (i, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let env: Envelope =
            serde_json::from_str(line).map_err(|e| format!("{name} line {}: {e}", i + 1))?;
        events.push(env);
    }
    if !events
        .iter()
        .any(|e| matches!(e.event, RunEvent::Started { .. }))
    {
        return Err(format!("{name} has no started event"));
    }
    Ok(events)
}

/// The records in `dir` that can be replayed, sorted by name. Unreadable ones are skipped.
pub fn list(dir: &Path) -> Vec<ReplayInfo> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "jsonl")
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| match read(dir, &name) {
            Ok(events) => Some(info(&name, &events)),
            Err(e) => {
                tracing::warn!(record = %name, error = %e, "not replayable");
                None
            }
        })
        .collect()
}

fn info(name: &str, events: &[Envelope]) -> ReplayInfo {
    let config = events.iter().find_map(|e| match &e.event {
        RunEvent::Started { config, .. } => Some(config.clone()),
        _ => None,
    });
    let ended = events.iter().find_map(|e| match &e.event {
        RunEvent::Ended { state, .. } => Some(*state),
        _ => None,
    });
    ReplayInfo {
        name: name.to_owned(),
        recorded_run: events.first().map(|e| e.run_id.clone()).unwrap_or_default(),
        ticks: u32::try_from(
            events
                .iter()
                .filter(|e| matches!(e.event, RunEvent::Tick { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX),
        baseline: config.as_ref().is_some_and(|c| c.baseline),
        start_s: config.as_ref().map_or(0.0, |c| c.start_s),
        drafted: config.as_ref().is_some_and(|c| !c.drafts.is_empty()),
        state: ended,
    }
}

/// The config a replay of `events` runs under: the recorded one, naming the record.
pub fn config(name: &str, events: &[Envelope]) -> Result<RunConfig, StartError> {
    let mut config = events
        .iter()
        .find_map(|e| match &e.event {
            RunEvent::Started { config, .. } => Some(config.clone()),
            _ => None,
        })
        .ok_or_else(|| StartError::Invalid(format!("{name} has no started event")))?;
    config.replay = Some(ReplaySource {
        record: name.to_owned(),
        recorded_run: events.first().map(|e| e.run_id.clone()).unwrap_or_default(),
    });
    Ok(config)
}

/// Sends the recorded events again, then an `ended` event with the recorded outcome (or
/// `stopped` if a stop was asked for).
pub async fn run(handle: Arc<RunHandle>, events: Vec<Envelope>, pace: std::time::Duration) {
    let stop = handle.stop_signal();
    let mut outcome = (RunState::Completed, None);
    let mut stopped = false;
    for env in events {
        let event = match env.event {
            RunEvent::Started { sequences, .. } => RunEvent::Started {
                config: handle.config.clone(),
                sequences,
            },
            RunEvent::Ended { state, message } => {
                outcome = (state, message);
                continue;
            }
            RunEvent::Tick { .. } => {
                if *stop.borrow() {
                    stopped = true;
                    break;
                }
                tokio::time::sleep(pace).await;
                handle.count_tick();
                env.event
            }
            other => other,
        };
        if matches!(
            event,
            RunEvent::Phase {
                phase: crate::run_dto::Phase::Running
            }
        ) {
            handle.set_state(RunState::Running);
        }
        handle.publish(env.sim_time_s, event);
    }
    let (state, message) = if stopped {
        (RunState::Stopped, None)
    } else {
        outcome
    };
    handle.set_state(state);
    handle.publish(None, RunEvent::Ended { state, message });
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::valid_name;

    #[test]
    fn record_names_cannot_walk_paths() {
        assert!(valid_name("run-1793082222-1"));
        assert!(valid_name("demo_day185.v2"));
        for bad in ["", "../secret", "a/b", ".hidden", "a\\b", "x y"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
