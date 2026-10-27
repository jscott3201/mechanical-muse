//! BOPTEST runtime host: runs verified sequences as a supervisory controller on a simulated
//! building, with the fail-safe the engine leaves to its host.
//!
//! Only CXF whose content id matches its verification receipt is loaded. Each tick: readings in,
//! one engine frame per sequence (in dependency order), clamp and gate the outputs, write
//! `<point>_u` / `<point>_activate`, advance.

#![forbid(unsafe_code)]

mod boptest;
mod plant;
pub mod preset;
mod sequence;
mod supervisor;
mod wiring;

pub use boptest::BoptestClient;
pub use plant::{FakePlant, Measurements, Plant, PlantError, Writes};
pub use sequence::{LoadError, RunningSequence, StepFailure};
pub use supervisor::{ConfigError, Event, FailSafe, Supervisor, TickReport};
pub use wiring::{InputBinding, OutputBinding, Scalar, Source, SourceError, TickOutputs};
