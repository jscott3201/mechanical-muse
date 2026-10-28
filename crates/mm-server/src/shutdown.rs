//! After the first Ctrl-C the server stops taking requests and waits for active runs to finish
//! their step and stop their BOPTEST test, so the worker is not left holding an orphan. Tokio's
//! signal handler replaces the default one for good, so without this a second Ctrl-C would do
//! nothing for the whole wait: `drain` gives up at once when `interrupt` fires and says which
//! BOPTEST test may need stopping by hand.

use std::future::Future;
use std::time::Duration;

use crate::runs::Runs;

/// A run that was still active when the server gave up waiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfinished {
    pub run: String,
    /// The BOPTEST test it selected; `None` for a replay or a run still selecting.
    pub testid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drained {
    /// No run is active any more.
    Done,
    /// The wait ran out with runs still active.
    TimedOut(Vec<Unfinished>),
    /// A second interrupt arrived with runs still active.
    Interrupted(Vec<Unfinished>),
}

/// The runs that are starting or running now.
pub fn unfinished(runs: &Runs) -> Vec<Unfinished> {
    runs.all_active()
        .into_iter()
        .map(|r| Unfinished {
            run: r.id.clone(),
            testid: r.testid(),
        })
        .collect()
}

/// Waits up to `limit` for every active run to end, or until `interrupt` completes.
pub async fn drain(runs: &Runs, limit: Duration, interrupt: impl Future<Output = ()>) -> Drained {
    let ended = async {
        while !runs.all_active().is_empty() {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    tokio::select! {
        () = ended => Drained::Done,
        () = tokio::time::sleep(limit) => Drained::TimedOut(unfinished(runs)),
        () = interrupt => match unfinished(runs) {
            left if left.is_empty() => Drained::Done,
            left => Drained::Interrupted(left),
        },
    }
}

/// Logs what `drain` left behind.
pub fn report(outcome: &Drained) {
    let (left, why) = match outcome {
        Drained::Done => return,
        Drained::TimedOut(left) => (left, "still active after the shutdown wait"),
        Drained::Interrupted(left) => (left, "still active at a second Ctrl-C; exiting now"),
    };
    for u in left {
        match &u.testid {
            Some(testid) => tracing::warn!(run = %u.run, %testid,
                "{why}; this BOPTEST test may need stopping (POST /stop/<testid>)"),
            None => tracing::warn!(run = %u.run, "{why}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::run_dto::RunRequest;
    use crate::runs::{PlantSource, RunsConfig};

    fn runs() -> Runs {
        Runs::new(RunsConfig {
            plant: PlantSource::Off,
            records: None,
            replays: None,
            replay_pace: Duration::ZERO,
        })
    }

    /// A run stuck in BOPTEST (registered, never ended) must not hold the exit for the whole
    /// wait once a second interrupt arrives, and the exit names its test.
    #[tokio::test]
    async fn a_second_interrupt_ends_the_wait_and_names_the_test() {
        let runs = runs();
        let stuck = runs.create(&RunRequest::default()).unwrap();
        stuck.set_testid("abc-123");
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            drain(&runs, Duration::from_secs(600), async {}),
        )
        .await
        .expect("the interrupt ends the wait");
        assert_eq!(
            outcome,
            Drained::Interrupted(vec![Unfinished {
                run: stuck.id.clone(),
                testid: Some("abc-123".into()),
            }])
        );
    }

    #[tokio::test]
    async fn the_wait_ends_at_its_limit_or_when_nothing_runs() {
        let runs = runs();
        assert_eq!(
            drain(&runs, Duration::from_secs(600), std::future::pending()).await,
            Drained::Done
        );
        let stuck = runs.create(&RunRequest::default()).unwrap();
        let outcome = drain(&runs, Duration::from_millis(50), std::future::pending()).await;
        assert_eq!(
            outcome,
            Drained::TimedOut(vec![Unfinished {
                run: stuck.id.clone(),
                testid: None,
            }])
        );
    }
}
