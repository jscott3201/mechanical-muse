//! The model-facing schema stays inside Meta's strict structured-output subset and agrees with the
//! pinned palette.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use mm_graph::{BLOCK_GRAPH_SCHEMA, BlockGraph, Palette};
use serde_json::Value;

fn schema() -> Value {
    serde_json::from_str(BLOCK_GRAPH_SCHEMA).unwrap()
}

#[test]
fn every_class_in_the_enum_is_authorable() {
    let palette = Palette::pinned().unwrap();
    let classes = schema()["$defs"]["block"]["properties"]["class"]["enum"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(classes.len(), 55);
    for class in classes {
        let class = class.as_str().unwrap();
        let spec = palette
            .class(class)
            .unwrap_or_else(|| panic!("{class} not in catalog"));
        assert!(spec.authorable, "{class} is not authorable");
    }
}

#[test]
fn objects_are_closed_and_fully_required() {
    fn walk(path: &str, v: &Value, depth: usize) {
        assert!(depth <= 10, "{path}: nesting deeper than 10");
        let Some(obj) = v.as_object() else { return };
        for banned in ["oneOf", "allOf", "pattern", "minItems", "maxItems", "not"] {
            assert!(!obj.contains_key(banned), "{path}: uses {banned}");
        }
        if obj.get("type").and_then(Value::as_str) == Some("object") {
            assert_eq!(
                obj.get("additionalProperties"),
                Some(&Value::Bool(false)),
                "{path}"
            );
            let mut props: Vec<_> = obj["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            let mut required: Vec<_> = obj["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_owned())
                .collect();
            props.sort();
            required.sort();
            assert_eq!(props, required, "{path}: every property must be required");
        }
        for (key, child) in obj {
            walk(&format!("{path}/{key}"), child, depth + 1);
            if let Some(items) = child.as_array() {
                for (i, item) in items.iter().enumerate() {
                    walk(&format!("{path}/{key}/{i}"), item, depth + 1);
                }
            }
        }
    }
    walk("#", &schema(), 0);
}

#[test]
fn golden_graphs_parse_as_block_graphs() {
    for name in ["ahu_0027", "ahu_0005"] {
        let path = format!(
            "{}/tests/golden/{name}.graph.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let _: BlockGraph = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    }
}
