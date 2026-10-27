//! MCP over stdio: one JSON-RPC 2.0 message per line in, one per line out.
//!
//! Dual-era: a client that opens with `initialize` (protocol 2025-11-25 and earlier) gets the
//! handshake; a client that sends per-request `_meta` (2026-07-28) is served statelessly and can
//! call `server/discover`. The server holds no per-connection state either way, so both eras
//! share one code path after the version check.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::tools::{self, Tools};

/// Modern revision first: it is what `server/discover` lists first.
pub const MODERN_VERSIONS: &[&str] = &["2026-07-28"];
/// Handshake-based revisions we answer `initialize` with; the first is the default.
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

// JSON-RPC and MCP error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_VERSION: i64 = -32022;

const INSTRUCTIONS: &str = "Mechanical Muse tools for authoring building control sequences as \
compact CDL block graphs. Workflow: list_specs, then get_spec for the task text, authoring \
rules and graph schema; palette for block classes and their ports; draft a block graph; \
validate_graph for fast structural findings; verify_graph to emit CXF, load it into the pinned \
open-control-engine and replay the spec's test vectors (passes X of Y scenarios). On a failure, \
read `feedback` (the same repair text our own agent gets), fix the graph and verify again. \
Every result is deterministic: the same graph gives the same content id and verdict.";

fn server_info() -> Value {
    json!({"name": "mm-mcp", "title": "Mechanical Muse", "version": env!("CARGO_PKG_VERSION")})
}

fn capabilities() -> Value {
    json!({"tools": {"listChanged": false}})
}

/// The MCP server: the tools plus the protocol around them.
pub struct Server {
    tools: Tools,
}

impl Server {
    pub fn new(tools: Tools) -> Self {
        Self { tools }
    }

    /// Reads messages until end of input, answering each request on its own line.
    pub fn serve<R: BufRead, W: Write>(&self, reader: R, mut writer: W) -> std::io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = self.handle_line(&line) {
                writer.write_all(reply.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
        }
        Ok(())
    }

    /// One incoming line to at most one outgoing line. Notifications and client responses get
    /// no reply.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(e) => {
                tracing::warn!(error = %e, "unparseable message");
                return Some(error(&Value::Null, PARSE_ERROR, "parse error", None));
            }
        };
        let Some(object) = message.as_object() else {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "expected one JSON-RPC object per line",
                None,
            ));
        };
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to a request we never send, or junk: nothing to answer.
            return None;
        };
        let Some(id) = object.get("id") else {
            tracing::debug!(method, "notification");
            return None;
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        Some(match self.request(method, &params) {
            Ok(result) => {
                serde_json::to_string(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
                    .unwrap_or_default()
            }
            Err(RpcError {
                code,
                message,
                data,
            }) => error(id, code, &message, data),
        })
    }

    fn request(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        if method == "initialize" {
            return Ok(initialize(params));
        }
        check_version(params)?;
        let result = match method {
            "server/discover" => {
                let mut versions: Vec<&str> = MODERN_VERSIONS.to_vec();
                versions.extend_from_slice(LEGACY_VERSIONS);
                json!({
                    "supportedVersions": versions,
                    "capabilities": capabilities(),
                    "_meta": {META_SERVER_INFO: server_info()},
                    "instructions": INSTRUCTIONS,
                })
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools": tools::definitions()}),
            "tools/call" => self.call(params)?,
            other => {
                return Err(RpcError::new(
                    METHOD_NOT_FOUND,
                    format!("method not found: {other}"),
                ));
            }
        };
        Ok(complete(result))
    }

    fn call(&self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call needs a tool name"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
        let outcome = self
            .tools
            .call(name, &arguments)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("unknown tool: {name}")))?;
        let (body, is_error) = match outcome {
            Ok(body) => (body, false),
            Err(e) => {
                tracing::info!(tool = name, code = %e.code, "tool error");
                (e.to_json(), true)
            }
        };
        let text = serde_json::to_string_pretty(&body).unwrap_or_default();
        Ok(json!({
            "content": [{"type": "text", "text": text}],
            "structuredContent": body,
            "isError": is_error,
        }))
    }
}

struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// Legacy handshake: echo the client's revision when we know it, else offer our newest legacy
/// one (the client then decides whether to continue).
fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| LEGACY_VERSIONS.contains(v) || MODERN_VERSIONS.contains(v))
        .or(LEGACY_VERSIONS.first().copied())
        .unwrap_or_default();
    json!({
        "protocolVersion": version,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

/// A request that names a protocol version in `_meta` must name one we serve. Requests without
/// one belong to a legacy session (the version was settled by `initialize`).
fn check_version(params: &Value) -> Result<(), RpcError> {
    let Some(requested) = params.pointer("/_meta").and_then(|m| m.get(META_VERSION)) else {
        return Ok(());
    };
    let known = requested
        .as_str()
        .is_some_and(|v| MODERN_VERSIONS.contains(&v) || LEGACY_VERSIONS.contains(&v));
    if known {
        return Ok(());
    }
    let mut supported: Vec<&str> = MODERN_VERSIONS.to_vec();
    supported.extend_from_slice(LEGACY_VERSIONS);
    Err(RpcError {
        code: UNSUPPORTED_VERSION,
        message: "Unsupported protocol version".into(),
        data: Some(json!({"supported": supported, "requested": requested})),
    })
}

/// Modern results say they are complete; legacy clients ignore the extra field.
fn complete(mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("resultType".into(), json!("complete"));
    }
    result
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> String {
    let mut body = json!({"code": code, "message": message});
    if let (Some(data), Some(object)) = (data, body.as_object_mut()) {
        object.insert("data".into(), data);
    }
    serde_json::to_string(&json!({"jsonrpc": "2.0", "id": id, "error": body})).unwrap_or_default()
}
