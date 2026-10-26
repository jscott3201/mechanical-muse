//! Constant baselines: a graph with the rule's interface whose outputs never change. Every model
//! number is reported beside these (`mm-benchmark-hygiene`).
//!
//! The engine drops boundary inputs nothing reads, so each input feeds a sink block whose output
//! goes nowhere; that keeps the inputs settable without affecting the outputs.

use mm_bridge::{ReplayLimits, ScenarioOutcome, replay};
use mm_graph::{Block, BlockGraph, EmitOptions, Palette, Param, ParamValue, SignalType, emit_cxf};
use serde::Serialize;

use crate::BenchError;
use crate::library::Rule;

/// Rules and scenarios a constant graph passes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Baseline {
    pub rules_passed: usize,
    pub scenarios_passed: usize,
    pub scenarios: usize,
}

/// A graph that outputs `value` forever (`false`/`true` on Boolean outputs, 0 on numeric ones).
pub fn baseline_graph(interface: &BlockGraph, value: bool) -> BlockGraph {
    let mut graph = BlockGraph {
        name: "baseline".into(),
        inputs: interface.inputs.clone(),
        outputs: interface.outputs.clone(),
        blocks: Vec::new(),
        connections: Vec::new(),
    };
    let param = |name: &str, value: ParamValue| Param {
        name: name.into(),
        value,
    };
    for (i, input) in interface.inputs.iter().enumerate() {
        let id = format!("sink{i}");
        let (class, params, port) = match input.ty {
            SignalType::Real => (
                "Reals.GreaterThreshold",
                vec![param("t", ParamValue::Number(0.0))],
                "u",
            ),
            SignalType::Int => (
                "Integers.GreaterThreshold",
                vec![param("t", ParamValue::Number(0.0))],
                "u",
            ),
            SignalType::Bool => ("Logical.Not", vec![], "u"),
        };
        graph.blocks.push(Block {
            id: id.clone(),
            class: class.into(),
            params,
        });
        graph
            .connections
            .push(format!("{} -> {id}.{port}", input.name));
    }
    for (i, output) in interface.outputs.iter().enumerate() {
        let id = format!("const{i}");
        let (class, k) = match output.ty {
            SignalType::Bool => ("Logical.Sources.Constant", ParamValue::Bool(value)),
            SignalType::Real => ("Reals.Sources.Constant", ParamValue::Number(0.0)),
            SignalType::Int => ("Integers.Sources.Constant", ParamValue::Number(0.0)),
        };
        graph.blocks.push(Block {
            id: id.clone(),
            class: class.into(),
            params: vec![param("k", k)],
        });
        graph.connections.push(format!("{id}.y -> {}", output.name));
    }
    graph
}

/// Constant-false and constant-true baselines over `rules`. A rule whose baseline cannot be
/// emitted or replayed is an error, never a silently smaller denominator.
pub fn baselines(rules: &[Rule], palette: &Palette) -> Result<(Baseline, Baseline), BenchError> {
    let run = |value: bool| -> Result<Baseline, BenchError> {
        let mut total = Baseline::default();
        for rule in rules {
            let graph = baseline_graph(&rule.reference, value);
            let cxf = emit_cxf(&graph, palette, &EmitOptions::default())
                .map_err(|e| BenchError::Other(format!("{} baseline: {e}", rule.id)))?;
            let report = replay(&cxf, &rule.vectors, &ReplayLimits::default())
                .map_err(|e| BenchError::Other(format!("{} baseline: {e}", rule.id)))?;
            total.scenarios += report.scenarios.len();
            total.scenarios_passed += report
                .scenarios
                .iter()
                .filter(|s| s.outcome == ScenarioOutcome::Pass)
                .count();
            total.rules_passed += usize::from(report.passed());
        }
        Ok(total)
    };
    Ok((run(false)?, run(true)?))
}
