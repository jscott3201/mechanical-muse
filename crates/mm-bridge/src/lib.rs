//! Engine host: CXF load receipts, DTOs, frame stepping and vector replay.
//!
//! This is the only crate that drives `oce-api`. Everything leaving it is a plain serde DTO, so
//! nothing upstream needs the engine's types (which have no `Serialize` or `PartialEq`).

#![forbid(unsafe_code)]

mod dto;
mod load;
mod names;
mod replay;
mod stepper;
mod vectors;

pub use dto::{DiagnosticDto, EngineErrorDto, WireValue};
pub use load::{ALLOWED_LOAD_WARNINGS, LoadFailure, Loaded, load_cxf};
pub use names::{NameError, resolve_name};
pub use replay::{
    ReplayError, ReplayLimits, ReplayReport, ScenarioOutcome, ScenarioReport, replay,
};
pub use stepper::{FrameStepper, StepError};
pub use vectors::{Clock, Expectation, InputSpec, Scalar, Scenario, Step, VectorFile};
