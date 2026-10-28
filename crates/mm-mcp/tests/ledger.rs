//! The ledger tools over the committed recorded runs, through the real binary on stdio.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn session(lines: &[Value]) -> BTreeMap<u64, Value> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_mm-mcp"))
        .env("MM_SPECS_DIR", format!("{root}/specs"))
        .env("MM_REPLAYS_DIR", format!("{root}/fixtures/replays"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .unwrap();
        for (i, args) in lines.iter().enumerate() {
            let (tool, arguments) = (args["tool"].clone(), args["arguments"].clone());
            let call = json!({"jsonrpc": "2.0", "id": i + 1, "method": "tools/call",
                "params": {"name": tool, "arguments": arguments}});
            writeln!(stdin, "{call}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            let reply: Value = serde_json::from_str(l).unwrap();
            (reply["id"].as_u64().unwrap(), reply)
        })
        .collect()
}

fn near(v: &Value, expected: f64) -> bool {
    (v.as_f64().unwrap() - expected).abs() < 0.5
}

#[test]
fn ledgers_and_comparisons_of_recorded_runs() {
    let replies = session(&[
        json!({"tool": "run_ledger", "arguments": {"record": "day143-morning-baseline"}}),
        json!({"tool": "compare_runs", "arguments": {
            "a": "day143-morning-baseline", "b": "replay:day143-morning-your-sequence-rev-b"}}),
        json!({"tool": "compare_runs", "arguments": {
            "a": "day143-morning-baseline", "b": "day185-noon-baseline"}}),
        json!({"tool": "run_ledger", "arguments": {"record": "../Cargo"}}),
        json!({"tool": "run_ledger", "arguments": {}}),
    ]);
    let ok = |id: u64| {
        let r = &replies[&id]["result"];
        assert_ne!(r["isError"], true, "{r}");
        r["structuredContent"].clone()
    };
    let err = |id: u64| {
        let r = &replies[&id]["result"];
        assert_eq!(r["isError"], true, "{r}");
        r["structuredContent"]["code"].as_str().unwrap().to_owned()
    };
    let l = ok(1);
    assert!(near(&l["scope1"]["kg_co2e"], 1644.0));
    assert!(near(&l["scope2"]["kg_co2e"], 2705.0));
    assert_eq!(l["reconciles"], true);
    assert_eq!(l["measured"], false);
    let c = ok(2);
    assert!(near(&c["b"]["scope1"]["kg_co2e"], 796.0));
    assert!(near(&c["delta"]["total"]["usd"], 694.97 - 963.53));
    assert_eq!(err(3), "window-mismatch");
    assert_eq!(err(4), "unknown-record");
    assert_eq!(err(5), "invalid-arguments");
}
