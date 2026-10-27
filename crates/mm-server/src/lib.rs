//! HTTP API and event stream for the web app. The binary (`main.rs`) only reads configuration
//! and serves [`app`]; everything else lives here so handler tests can drive the router directly.

#![forbid(unsafe_code)]

pub mod dto;
mod routes;
pub mod run_dto;
mod run_routes;
pub mod runs;
mod session;
pub mod specs;

pub use routes::{AppState, ENGINE_REV, app};
pub use runs::{PlantSource, RunsConfig};
pub use specs::{SpecError, SpecStore};
