//! An MCP server (stdio) that lets coding agents, such as Meta's Muse Code or any MCP client,
//! author control sequences with our own tools: list and read specs, browse the block palette,
//! validate a block graph, emit CXF, and verify a graph by replaying a spec's test vectors on
//! the pinned engine.
//!
//! No network and no model calls: the client's agent is the model. Results are deterministic
//! (AGENTS.md hard rule 6). Logs go to stderr; stdout carries only protocol messages.

#![forbid(unsafe_code)]

mod checks;
mod rpc;
mod tools;

pub use rpc::{LEGACY_VERSIONS, MODERN_VERSIONS, Server};
pub use tools::{ToolError, ToolResult, Tools, definitions};
