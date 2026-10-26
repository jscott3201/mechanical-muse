//! Shared helpers for mm-graph integration tests.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn library_fixture(id: &str) -> Vec<u8> {
    let path = repo_root().join(format!(
        "fixtures/open-control-library/{id}/rule.cxf.jsonld"
    ));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Order-insensitive form of a CXF document: nodes keyed by `@id`; a lone reference object equals
/// a one-element array; reference lists compare as sets; doubles compare by value, not spelling.
pub fn canonical(bytes: &[u8]) -> (Value, BTreeMap<String, Value>) {
    let doc: Value = serde_json::from_slice(bytes).expect("CXF is JSON");
    let mut nodes = BTreeMap::new();
    for node in doc["@graph"].as_array().expect("@graph array") {
        let id = node["@id"].as_str().expect("@id").to_owned();
        let mut out = serde_json::Map::new();
        for (key, value) in node.as_object().expect("node object") {
            out.insert(key.clone(), normalize(key, value));
        }
        nodes.insert(id, Value::Object(out));
    }
    (doc["@context"].clone(), nodes)
}

fn normalize(key: &str, value: &Value) -> Value {
    let is_ref = |v: &Value| {
        v.as_object()
            .is_some_and(|o| o.len() == 1 && o.contains_key("@id"))
    };
    match value {
        Value::Object(_) if is_ref(value) && key != "S231:isOfDataType" => {
            Value::Array(vec![value.clone()])
        }
        Value::Array(items) => {
            let mut items = items.clone();
            items.sort_by_key(|v| v.to_string());
            Value::Array(items)
        }
        Value::Object(o)
            if key == "S231:value"
                && o["@type"].as_str().is_some_and(|t| t.ends_with("#double")) =>
        {
            let x: f64 = o["@value"]
                .as_str()
                .expect("literal")
                .parse()
                .expect("double");
            let mut o = o.clone();
            o.insert(
                "@value".into(),
                Value::String(format!("{:016x}", x.to_bits())),
            );
            Value::Object(o)
        }
        other => other.clone(),
    }
}
