//! Drives `mm-mcp` the way an MCP client does: the real binary over stdio pipes, one process
//! per test. Offline; no model calls.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn specs_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../specs"))
}

fn reference_graph(spec: &str) -> Value {
    let text = std::fs::read_to_string(specs_dir().join(spec).join("graph.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn request(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn call(id: u64, tool: &str, arguments: Value) -> String {
    request(
        id,
        "tools/call",
        json!({"name": tool, "arguments": arguments}),
    )
}

/// Sends every line to the binary, closes stdin, and returns the replies by id.
fn session(lines: &[String]) -> BTreeMap<u64, Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mm-mcp"))
        .env("MM_SPECS_DIR", specs_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for line in lines {
            writeln!(stdin, "{line}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "server exited with {}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .map(|line| {
            let reply: Value = serde_json::from_str(line).expect("every stdout line is JSON");
            (reply["id"].as_u64().unwrap(), reply)
        })
        .collect()
}

fn structured(reply: &Value) -> &Value {
    &reply["result"]["structuredContent"]
}

#[test]
fn a_client_session_over_stdio() {
    let chw = reference_graph("chw_plant_reset");
    // A loadable graph with one wrong timing: the start delay is half what the task asks for.
    let mut mistimed = chw.clone();
    for block in mistimed["blocks"].as_array_mut().unwrap() {
        if block["id"] == "startDelay" {
            block["params"][0] = json!({"name": "delayTime", "value": 600.0});
        }
    }
    // A graph with an output nobody drives.
    let mut undriven = chw.clone();
    undriven["connections"]
        .as_array_mut()
        .unwrap()
        .retain(|c| !c.as_str().unwrap().ends_with("-> dpSet"));

    let replies = session(&[
        request(
            1,
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "0"}}),
        ),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        request(2, "tools/list", json!({})),
        call(
            3,
            "verify_graph",
            json!({"spec": "chw_plant_reset", "graph": chw}),
        ),
        call(
            4,
            "verify_graph",
            json!({"spec": "chw_plant_reset", "graph": mistimed}),
        ),
        call(
            5,
            "verify_graph",
            json!({"spec": "chw_plant_reset", "graph": undriven}),
        ),
        request(6, "ping", json!({})),
    ]);
    // The notification got no reply.
    assert_eq!(
        replies.keys().copied().collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );

    let init = &replies[&1]["result"];
    assert_eq!(init["protocolVersion"], "2025-11-25");
    assert!(init["capabilities"]["tools"].is_object());
    assert_eq!(init["serverInfo"]["name"], "mm-mcp");

    let names: Vec<&str> = replies[&2]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "list_specs",
            "get_spec",
            "palette",
            "validate_graph",
            "emit_cxf",
            "verify_graph"
        ]
    );

    let pass = structured(&replies[&3]);
    assert_eq!(replies[&3]["result"]["isError"], false);
    assert_eq!(pass["passed"], true, "{pass}");
    assert_eq!(pass["passed_scenarios"], pass["total_scenarios"]);
    assert!(pass["total_scenarios"].as_u64().unwrap() > 0);
    assert!(pass["content_id"].as_str().unwrap().starts_with("cxf:"));
    assert_eq!(pass["feedback"], Value::Null);
    // The text block carries the same JSON, for clients that ignore structuredContent.
    let text = replies[&3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert_eq!(&serde_json::from_str::<Value>(text).unwrap(), pass);

    let fail = structured(&replies[&4]);
    assert_eq!(fail["passed"], false);
    assert_eq!(fail["stage"], "replay");
    assert_eq!(fail["code"], "replay-failed");
    assert!(fail["passed_scenarios"].as_u64().unwrap() < fail["total_scenarios"].as_u64().unwrap());
    let failing = fail["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["result"] == "fail")
        .expect("a failing scenario");
    assert!(failing["t"].is_number() && failing["expected"].is_number());
    let feedback = fail["feedback"].as_str().unwrap();
    assert!(feedback.contains("test scenarios failed"), "{feedback}");

    let invalid = structured(&replies[&5]);
    assert_eq!(invalid["passed"], false);
    assert_eq!(invalid["stage"], "validate");
    assert_eq!(invalid["code"], "undriven");
    assert!(invalid["feedback"].as_str().unwrap().contains("undriven"));

    // Handshake-era replies carry no resultType; the SDK strips it for those revisions.
    assert_eq!(replies[&6]["result"], json!({}));
}

/// A handshake-era session: `initialize` and `initialized`, then `lines` (ids from 10 up).
fn legacy(lines: &[String]) -> BTreeMap<u64, Value> {
    let mut all = vec![
        request(
            1,
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "0"}}),
        ),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
    ];
    all.extend_from_slice(lines);
    session(&all)
}

#[test]
fn an_unknown_class_is_a_typed_finding() {
    let mut graph = reference_graph("chw_plant_reset");
    graph["blocks"][0]["class"] = json!("Logical.MadeUpBlock");
    let replies = legacy(&[
        call(
            10,
            "validate_graph",
            json!({"graph": graph, "spec": "chw_plant_reset"}),
        ),
        call(11, "emit_cxf", json!({"graph": graph})),
    ]);
    let body = structured(&replies[&10]);
    assert_eq!(body["ok"], false);
    assert_eq!(body["code"], "unknown-class");
    assert_eq!(body["findings"][0]["code"], "unknown-class");
    assert_eq!(body["findings"][0]["subject"], graph["blocks"][0]["id"]);

    // emit_cxf refuses the same graph as a tool error carrying the same code.
    assert_eq!(replies[&11]["result"]["isError"], true);
    assert_eq!(structured(&replies[&11])["code"], "unknown-class");
}

#[test]
fn emitting_is_deterministic_and_matches_verify() {
    let graph = reference_graph("ahu_chw_requests");
    let args = json!({"graph": graph, "spec": "ahu_chw_requests"});
    let first = legacy(&[
        call(10, "emit_cxf", args.clone()),
        call(11, "verify_graph", args.clone()),
    ]);
    // A second process: the same graph gives the same bytes and content id.
    let second = legacy(&[call(10, "emit_cxf", args)]);
    assert_eq!(first[&10], second[&10]);
    let emitted = structured(&first[&10]);
    assert_eq!(emitted["content_id"], structured(&first[&11])["content_id"]);
    assert!(emitted["cxf"].as_str().unwrap().contains("@context"));
}

#[test]
fn specs_and_palette_are_listed() {
    let replies = legacy(&[
        call(10, "list_specs", json!({})),
        call(11, "get_spec", json!({"spec": "chw_plant_reset"})),
        call(12, "palette", json!({"query": "TRUEDELAY"})),
        call(13, "get_spec", json!({"spec": "nope"})),
        call(
            14,
            "verify_graph",
            json!({"spec": "chw_plant_reset", "graph": 3}),
        ),
    ]);
    let specs = structured(&replies[&10])["specs"].as_array().unwrap();
    assert!(specs.iter().any(|s| s["name"] == "chw_plant_reset"));

    let spec = structured(&replies[&11]);
    assert!(
        spec["task"]
            .as_str()
            .unwrap()
            .contains("Sequence: chw_plant_reset")
    );
    assert!(spec["graph_schema"]["properties"]["blocks"].is_object());
    // Scenario names and descriptions only; expected values stay with the verifier.
    assert!(spec["scenarios"][0]["name"].is_string());
    assert!(spec["scenarios"][0].get("expect").is_none());

    assert_eq!(structured(&replies[&12])["count"], 1);

    assert_eq!(replies[&13]["result"]["isError"], true);
    assert_eq!(structured(&replies[&13])["code"], "unknown-spec");

    assert_eq!(replies[&14]["result"]["isError"], true);
    assert_eq!(structured(&replies[&14])["code"], "not-a-graph");
}

/// Per-request metadata for the 2026-07-28 revision, which has no handshake.
fn modern_meta(version: &str) -> Value {
    json!({"_meta": {
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    }})
}

#[test]
fn a_modern_client_needs_no_handshake() {
    let mut verify = modern_meta("2026-07-28");
    verify["name"] = json!("verify_graph");
    verify["arguments"] =
        json!({"spec": "chw_plant_reset", "graph": reference_graph("chw_plant_reset")});
    let replies = session(&[
        request(1, "server/discover", modern_meta("2026-07-28")),
        request(2, "tools/list", modern_meta("1900-01-01")),
        request(3, "tools/call", verify),
    ]);
    let discover = &replies[&1]["result"];
    let versions = discover["supportedVersions"].as_array().unwrap();
    assert!(versions.contains(&json!("2026-07-28")));
    assert!(versions.contains(&json!("2025-11-25")));
    assert_eq!(discover["resultType"], "complete");

    assert_eq!(replies[&2]["error"]["code"], -32022);
    assert_eq!(replies[&2]["error"]["data"]["requested"], "1900-01-01");

    assert_eq!(replies[&3]["result"]["resultType"], "complete");
    assert_eq!(structured(&replies[&3])["passed"], true);
}

#[test]
fn protocol_errors_keep_the_session_alive() {
    let replies = legacy(&[
        call(10, "no_such_tool", json!({})),
        request(11, "bogus/method", json!({})),
        // A line that is not JSON gets no reply and does not end the session.
        "{not json".to_owned(),
        request(12, "ping", json!({})),
    ]);
    assert_eq!(replies[&10]["error"]["code"], -32602);
    assert_eq!(replies[&11]["error"]["code"], -32601);
    assert_eq!(replies[&12]["result"], json!({}));

    // An unknown handshake revision is answered with the newest one that has a handshake.
    let replies = session(&[request(
        1,
        "initialize",
        json!({"protocolVersion": "2020-01-01", "capabilities": {},
               "clientInfo": {"name": "test", "version": "0"}}),
    )]);
    assert_eq!(replies[&1]["result"]["protocolVersion"], "2025-11-25");
}
