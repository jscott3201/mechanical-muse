//! Re-emits a library CXF file through the compact graph and prints the result.
//! Usage: `cargo run -p mm-graph --example roundtrip -- <rule.cxf.jsonld>`

use std::io::Write;

use mm_graph::{EmitOptions, Palette, cxf_to_graph, emit_cxf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: roundtrip <rule.cxf.jsonld>")?;
    let imported = cxf_to_graph(&std::fs::read(path)?)?;
    let options = EmitOptions {
        namespace: Some(imported.namespace),
        root_label: Some(imported.root_label),
    };
    let bytes = emit_cxf(&imported.graph, &Palette::pinned()?, &options)?;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}
