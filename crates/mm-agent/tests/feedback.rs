//! The repair feedback names the classes that break an algebraic loop. This pins that claim to
//! the engine: if a revision changes which classes have no direct feedthrough, this fails.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use mm_bridge::load_cxf;
use mm_graph::{BlockGraph, EmitOptions, Palette, emit_cxf};

/// `xin -> op.u1`, `op.y -> brk.u -> brk.y -> op.u2`: a loop through `class`.
fn loop_through(signal: &str, op: &str, class: &str, params: &str) -> Result<(), String> {
    let graph: BlockGraph = serde_json::from_str(&format!(
        r#"{{"name":"probe",
            "inputs":[{{"name":"xin","type":"{signal}","unit":"1","description":""}}],
            "outputs":[{{"name":"xout","type":"{signal}","description":""}}],
            "blocks":[{{"id":"op","class":"{op}","params":[]}},
                      {{"id":"brk","class":"{class}","params":[{params}]}}],
            "connections":["xin -> op.u1","op.y -> brk.u","brk.y -> op.u2","op.y -> xout"]}}"#
    ))
    .unwrap();
    let cxf = emit_cxf(&graph, &Palette::pinned().unwrap(), &EmitOptions::default())
        .map_err(|e| format!("{class}: {e}"))?;
    load_cxf(&cxf).map(|_| ()).map_err(|f| f.code)
}

#[test]
fn only_unit_delay_and_pre_break_a_loop() {
    let period = r#"{"name":"samplePeriod","value":60}"#;
    assert_eq!(
        loop_through("real", "Reals.Add", "Discrete.UnitDelay", period),
        Ok(())
    );
    assert_eq!(
        loop_through("bool", "Logical.Or", "Logical.Pre", ""),
        Ok(())
    );
    for (signal, op, class, params) in [
        ("real", "Reals.Add", "Discrete.Sampler", period),
        ("real", "Reals.Add", "Discrete.ZeroOrderHold", period),
        (
            "bool",
            "Logical.Or",
            "Logical.TrueDelay",
            r#"{"name":"delayTime","value":60}"#,
        ),
        (
            "bool",
            "Logical.Or",
            "Logical.TrueFalseHold",
            r#"{"name":"trueHoldDuration","value":60}"#,
        ),
    ] {
        assert_eq!(
            loop_through(signal, op, class, params),
            Err("algebraic-loop".to_owned()),
            "{class} should pass its input through"
        );
    }
}
