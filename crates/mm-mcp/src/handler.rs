//! The MCP side, on the official Rust SDK (`rmcp`): server identity, the tool list, and
//! `tools/call` routed to [`Tools`]. Protocol versions, the handshake, `server/discover`, ping
//! and JSON-RPC framing are the SDK's.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;

use crate::tools::{Tools, definitions};

const INSTRUCTIONS: &str = "Mechanical Muse tools for authoring building control sequences as \
compact CDL block graphs. Workflow: list_specs, then get_spec for the task text, authoring \
rules and graph schema; palette for block classes and their ports; draft a block graph; \
validate_graph for fast structural findings; verify_graph to emit CXF, load it into the pinned \
open-control-engine and replay the spec's test vectors (passes X of Y scenarios). On a failure, \
read `feedback` (the same repair text our own agent gets), fix the graph and verify again. \
Every result is deterministic: the same graph gives the same content id and verdict.";

/// The MCP server: our tools behind the SDK's handler trait.
#[derive(Clone)]
pub struct McpServer {
    tools: Arc<Tools>,
    list: Arc<Vec<Tool>>,
}

/// The tool definitions did not fit the SDK's `Tool` type (a bug in `definitions`).
#[derive(Debug, thiserror::Error)]
#[error("tool definitions are not valid MCP tools: {0}")]
pub struct DefinitionError(#[from] serde_json::Error);

impl McpServer {
    pub fn new(tools: Tools) -> Result<Self, DefinitionError> {
        let list: Vec<Tool> = serde_json::from_value(definitions())?;
        Ok(Self {
            tools: Arc::new(tools),
            list: Arc::new(list),
        })
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = Implementation::new("mm-mcp", env!("CARGO_PKG_VERSION"));
        info.title = Some("Mechanical Muse".into());
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(info)
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.list.as_ref().clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.into_owned();
        if !self.list.iter().any(|t| t.name == name) {
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {name}"),
                None,
            ));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let tools = Arc::clone(&self.tools);
        // Replays are CPU-bound: keep them off the runtime that reads stdin.
        let tool = name.clone();
        let outcome = tokio::task::spawn_blocking(move || tools.call(&tool, &arguments))
            .await
            .map_err(|e| {
                tracing::error!(tool = %name, error = %e, "tool task failed");
                ErrorData::internal_error("tool stopped unexpectedly", None)
            })?
            .ok_or_else(|| ErrorData::invalid_params(format!("unknown tool: {name}"), None))?;
        let (body, is_error) = match outcome {
            Ok(body) => (body, false),
            Err(e) => {
                tracing::info!(tool = %name, code = %e.code, "tool error");
                (e.to_json(), true)
            }
        };
        let text = serde_json::to_string_pretty(&body).unwrap_or_default();
        let mut result = if is_error {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        };
        result.structured_content = Some(body);
        Ok(result.into())
    }
}
