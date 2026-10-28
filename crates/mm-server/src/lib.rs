//! HTTP API and event stream for the web app. The binary (`main.rs`) only reads configuration
//! and serves [`app`]; everything else lives here so handler tests can drive the router directly.

#![forbid(unsafe_code)]

pub mod author_dto;
mod author_routes;
pub mod authoring;
pub mod bacnet;
pub mod bind;
pub mod bind_routes;
pub mod drafts;
pub mod dto;
pub mod explain;
mod journal;
pub mod ledger;
mod ledger_routes;
mod recording;
mod replay;
mod routes;
pub mod run_dto;
pub mod run_explain;
mod run_facts;
mod run_routes;
pub mod runs;
mod session;
pub mod specs;
mod web;
pub mod workbench;
mod workbench_draft;

pub use authoring::AuthoringConfig;
pub use routes::{AppState, ENGINE_REV, app};
pub use runs::{PlantSource, RunsConfig};
pub use specs::{SpecError, SpecStore};
