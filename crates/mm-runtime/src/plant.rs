//! The building the supervisor talks to: BOPTEST in production, a scripted fake in tests.

use std::collections::BTreeMap;
use std::future::Future;

use serde::Serialize;

/// One step's readings, keyed by BOPTEST point name, plus `time` (simulation seconds).
pub type Measurements = BTreeMap<String, f64>;

/// What to send with the next advance: `<point>_u` values and `<point>_activate` flags.
pub type Writes = BTreeMap<String, f64>;

#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlantError {
    #[error("plant request failed: {message}")]
    Transport { message: String },
    #[error("plant refused the request ({status}): {message}")]
    Refused { status: u16, message: String },
    #[error("plant reply was not understood: {message}")]
    Malformed { message: String },
    #[error("the scenario has ended")]
    Ended,
}

/// A building that advances one control step at a time.
pub trait Plant {
    /// Applies `writes`, advances one step, and returns the new readings.
    fn advance(
        &mut self,
        writes: &Writes,
    ) -> impl Future<Output = Result<Measurements, PlantError>> + Send;
}

/// A scripted plant for tests and offline demos: returns the readings it was given, in order,
/// and records every write it receives.
#[derive(Debug, Default)]
pub struct FakePlant {
    pub script: Vec<Measurements>,
    pub writes: Vec<Writes>,
    next: usize,
}

impl FakePlant {
    pub fn new(script: Vec<Measurements>) -> Self {
        Self {
            script,
            writes: Vec::new(),
            next: 0,
        }
    }
}

impl Plant for FakePlant {
    async fn advance(&mut self, writes: &Writes) -> Result<Measurements, PlantError> {
        self.writes.push(writes.clone());
        let reading = self
            .script
            .get(self.next)
            .cloned()
            .ok_or(PlantError::Ended)?;
        self.next += 1;
        Ok(reading)
    }
}
