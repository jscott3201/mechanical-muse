//! An MCP server (stdio) that lets coding agents, such as Meta's Muse Code or any MCP client,
//! author control sequences with our own tools: list and read specs, browse the block palette,
//! validate a block graph, emit CXF, and verify a graph by replaying a spec's test vectors on
//! the pinned engine. The protocol side is the official Rust SDK (`rmcp`).
//!
//! No network and no model calls: the client's agent is the model. Results are deterministic
//! (AGENTS.md hard rule 6). Logs go to stderr; stdout carries only protocol messages.

#![forbid(unsafe_code)]

mod checks;
mod handler;
mod tools;

pub use handler::{DefinitionError, McpServer};
pub use tools::{ToolError, ToolResult, Tools, definitions};
