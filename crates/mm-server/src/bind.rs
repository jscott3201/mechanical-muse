//! How an engineer's sequence connects to BOPTEST: each input fed by a measurement (or a
//! threshold on one, or a constant), each output written to a setpoint within limits. Muse Spark
//! proposes a binding; this module checks every point against BOPTEST's own point list and the
//! sequence's interface, and the engineer confirms it before any run uses it. Points that exist
//! once per floor are written with `{floor}`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use mm_agent::TaskSpec;
use mm_graph::SignalType;
use mm_runtime::{InputBinding, OutputBinding, Scalar, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

const POINTS_CSV: &str = include_str!("../../../docs/boptest-points.csv");
pub const FLOORS: [u8; 3] = [1, 2, 3];

/// One BOPTEST point from `docs/boptest-points.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub name: String,
    /// `measurement` or `input`.
    pub kind: String,
    pub unit: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub description: String,
}

/// Splits one CSV line, honouring double quotes.
fn fields(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(String::new()),
            _ => {
                if let Some(last) = out.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    out
}

/// BOPTEST's points, by name (inputs without their `_activate` twins).
pub static POINTS: LazyLock<BTreeMap<String, Point>> = LazyLock::new(|| {
    POINTS_CSV
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f = fields(line);
            let get = |i: usize| f.get(i).cloned().unwrap_or_default();
            let name = get(0);
            if name.is_empty() || name.ends_with("_activate") {
                return None;
            }
            Some((
                name.clone(),
                Point {
                    name,
                    kind: get(1),
                    unit: get(2),
                    min: get(3).parse().ok(),
                    max: get(4).parse().ok(),
                    description: get(5),
                },
            ))
        })
        .collect()
});

/// The point list as the model sees it, one line per point.
pub fn listing() -> String {
    POINTS
        .values()
        .map(|p| {
            let range = match (p.min, p.max) {
                (Some(lo), Some(hi)) => format!("{lo}..{hi}"),
                _ => String::new(),
            };
            format!(
                "{} | {} | {} | {} | {}",
                p.name, p.kind, p.unit, range, p.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InputBind {
    pub input: String,
    /// `measurement`, `above` or `constant`.
    pub kind: String,
    /// A measurement point (may contain `{floor}`); empty for a constant.
    pub point: String,
    /// For `above`: true while the reading is above this, in the point's unit.
    pub threshold: f64,
    /// For `constant`.
    #[ts(type = "number | boolean")]
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputBind {
    pub output: String,
    /// The setpoint written, without `_u` (may contain `{floor}`); empty when not sent.
    pub point: String,
    /// The measurement that shows the result, for the charts; may be empty.
    pub measured: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Binding {
    pub floors: Vec<u8>,
    pub inputs: Vec<InputBind>,
    pub outputs: Vec<OutputBind>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn on_floor(template: &str, floor: u8) -> String {
    template.replace("{floor}", &floor.to_string())
}

fn measurement(template: &str, floor: u8) -> Option<&'static Point> {
    POINTS
        .get(&on_floor(template, floor))
        .filter(|p| p.kind == "measurement")
}

/// Problems that stop `binding` from running `task` on BOPTEST; empty when it can run.
pub fn check(binding: &Binding, task: &TaskSpec) -> Vec<String> {
    let mut problems = Vec::new();
    let floors: BTreeSet<u8> = binding.floors.iter().copied().collect();
    if floors.is_empty() || !floors.iter().all(|f| FLOORS.contains(f)) {
        problems.push("choose one or more of floors 1, 2 and 3".into());
    }
    let types: BTreeMap<&str, SignalType> = task
        .inputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty))
        .collect();
    let mut bound = BTreeSet::new();
    for b in &binding.inputs {
        let Some(&ty) = types.get(b.input.as_str()) else {
            problems.push(format!("`{}` is not an input of this sequence", b.input));
            continue;
        };
        if !bound.insert(b.input.as_str()) {
            problems.push(format!("input `{}` is bound twice", b.input));
        }
        let unknown = |problems: &mut Vec<String>| {
            for f in &floors {
                if measurement(&b.point, *f).is_none() {
                    problems.push(format!(
                        "`{}` (input `{}`, floor {f}) is not a BOPTEST measurement",
                        on_floor(&b.point, *f),
                        b.input
                    ));
                    break;
                }
            }
        };
        match (b.kind.as_str(), ty) {
            ("measurement", SignalType::Real) => unknown(&mut problems),
            ("above", SignalType::Bool) => {
                unknown(&mut problems);
                if !b.threshold.is_finite() {
                    problems.push(format!("input `{}` needs a finite threshold", b.input));
                }
            }
            ("constant", _) => {
                if constant(&b.value, ty).is_none() {
                    problems.push(format!("input `{}` needs a constant of its type", b.input));
                }
            }
            (kind, ty) => problems.push(format!(
                "input `{}` ({ty:?}) cannot be fed as `{kind}`: a real takes a measurement, a \
                 boolean a threshold on one, and either can be a constant",
                b.input
            )),
        }
    }
    for p in &task.inputs {
        if !bound.contains(p.name.as_str()) {
            problems.push(format!("input `{}` is not bound", p.name));
        }
    }
    let outputs: BTreeMap<&str, SignalType> = task
        .outputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty))
        .collect();
    let mut written = BTreeSet::new();
    for b in &binding.outputs {
        let Some(&ty) = outputs.get(b.output.as_str()) else {
            problems.push(format!("`{}` is not an output of this sequence", b.output));
            continue;
        };
        if !b.measured.is_empty()
            && floors
                .iter()
                .any(|f| measurement(&b.measured, *f).is_none())
        {
            problems.push(format!("`{}` is not a BOPTEST measurement", b.measured));
        }
        if b.point.is_empty() {
            continue;
        }
        if ty != SignalType::Real {
            problems.push(format!(
                "output `{}` is not a real, so it can only be shown, not written",
                b.output
            ));
            continue;
        }
        if floors.len() > 1 && !b.point.contains("{floor}") {
            problems.push(format!(
                "output `{}` would write the same point `{}` from every floor",
                b.output, b.point
            ));
        }
        for f in &floors {
            let name = on_floor(&b.point, *f);
            if !written.insert(name.clone()) {
                problems.push(format!("`{name}` is written twice"));
            }
            match POINTS
                .get(&format!("{name}_u"))
                .filter(|p| p.kind == "input")
            {
                None => problems.push(format!("`{name}` is not a BOPTEST setpoint")),
                Some(p) => {
                    let lo = p.min.unwrap_or(f64::NEG_INFINITY);
                    let hi = p.max.unwrap_or(f64::INFINITY);
                    if !(b.min.is_finite() && b.max.is_finite() && b.min < b.max)
                        || b.min < lo
                        || b.max > hi
                    {
                        problems.push(format!(
                            "output `{}`: limits {}..{} must be finite, ordered and within \
                             `{name}`'s range {lo}..{hi}",
                            b.output, b.min, b.max
                        ));
                    }
                }
            }
        }
    }
    problems.dedup();
    problems
}

fn constant(value: &Value, ty: SignalType) -> Option<Scalar> {
    match (ty, value) {
        (SignalType::Bool, Value::Bool(b)) => Some(Scalar::Bool(*b)),
        (SignalType::Real, Value::Number(n)) => {
            n.as_f64().filter(|x| x.is_finite()).map(Scalar::Real)
        }
        (SignalType::Int, Value::Number(n)) => n.as_i64().map(Scalar::Int),
        _ => None,
    }
}

/// The sequence instance name on `floor`.
pub fn instance(spec: &str, floor: u8) -> String {
    format!("{spec}_floor{floor}")
}

/// The runtime bindings for one floor, for a binding [`check`] accepted.
pub fn on(
    binding: &Binding,
    task: &TaskSpec,
    spec: &str,
    floor: u8,
) -> (Vec<InputBinding>, Vec<(String, OutputBinding)>) {
    let types: BTreeMap<&str, SignalType> = task
        .inputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty))
        .collect();
    let inputs = binding
        .inputs
        .iter()
        .filter_map(|b| {
            let ty = *types.get(b.input.as_str())?;
            let source = match b.kind.as_str() {
                "measurement" => Source::Measurement {
                    point: on_floor(&b.point, floor),
                },
                "above" => Source::MeasurementAbove {
                    point: on_floor(&b.point, floor),
                    threshold: b.threshold,
                },
                _ => Source::Constant {
                    value: constant(&b.value, ty)?,
                },
            };
            Some(InputBinding {
                input: b.input.clone(),
                source,
            })
        })
        .collect();
    let outputs = binding
        .outputs
        .iter()
        .filter(|b| !b.point.is_empty())
        .map(|b| {
            (
                instance(spec, floor),
                OutputBinding {
                    output: b.output.clone(),
                    point: on_floor(&b.point, floor),
                    min: b.min,
                    max: b.max,
                    gate: None,
                },
            )
        })
        .collect();
    (inputs, outputs)
}

/// Readings to show for a bound sequence: what feeds it and what shows its result.
pub fn watch(binding: &Binding) -> Vec<String> {
    let mut points = BTreeSet::new();
    for f in &binding.floors {
        for b in &binding.inputs {
            if b.kind != "constant" {
                points.insert(on_floor(&b.point, *f));
            }
        }
        for b in &binding.outputs {
            if !b.measured.is_empty() {
                points.insert(on_floor(&b.measured, *f));
            }
        }
    }
    points.into_iter().collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use serde_json::json;

    use super::*;

    fn task() -> TaskSpec {
        serde_json::from_value(json!({
            "name": "sat", "description": "",
            "inputs": [
                {"name": "ena", "type": "bool", "unit": "1", "meaning": "AHU on"},
                {"name": "oat", "type": "real", "unit": "K", "meaning": "Outdoor air"}
            ],
            "outputs": [
                {"name": "sp", "type": "real", "unit": "K", "meaning": "Setpoint"},
                {"name": "flag", "type": "bool", "unit": "1", "meaning": "Shown only"}
            ]
        }))
        .unwrap()
    }

    fn binding() -> Binding {
        serde_json::from_value(json!({
            "floors": [1, 2, 3],
            "inputs": [
                {"input": "ena", "kind": "above", "point": "floor{floor}_reaAHU_occ_y", "threshold": 0.5, "value": 0},
                {"input": "oat", "kind": "measurement", "point": "weaSta_reaWeaTDryBul_y", "threshold": 0, "value": 0}
            ],
            "outputs": [
                {"output": "sp", "point": "floor{floor}_TSupAirSet", "measured": "floor{floor}_reaAHU_TSup_y", "min": 285.15, "max": 300.0},
                {"output": "flag", "point": "", "measured": "", "min": 0, "max": 0}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn the_point_list_parses_quoted_descriptions() {
        let occ = &POINTS["floor1_reaAHU_occ_y"];
        assert_eq!(occ.kind, "measurement");
        assert!(occ.description.contains("1 occupied, 0 unoccupied"));
        assert_eq!(POINTS["floor1_TSupAirSet_u"].min, Some(285.15));
        assert!(!POINTS.contains_key("floor1_TSupAirSet_activate"));
    }

    #[test]
    fn a_sound_binding_runs_on_each_floor() {
        assert_eq!(check(&binding(), &task()), Vec::<String>::new());
        let (inputs, outputs) = on(&binding(), &task(), "sat", 2);
        assert_eq!(
            inputs[0].source,
            Source::MeasurementAbove {
                point: "floor2_reaAHU_occ_y".into(),
                threshold: 0.5
            }
        );
        assert_eq!(outputs.len(), 1, "the display-only flag is not written");
        assert_eq!(outputs[0].0, "sat_floor2");
        assert_eq!(outputs[0].1.point, "floor2_TSupAirSet");
        assert!(watch(&binding()).contains(&"floor3_reaAHU_TSup_y".to_owned()));
    }

    #[test]
    fn bad_bindings_are_refused_with_reasons() {
        let mut b = binding();
        b.inputs[1].point = "floor{floor}_nope_y".into();
        b.outputs[0].max = 400.0;
        b.outputs[1].point = "floor{floor}_TSupAirSet".into();
        b.inputs.pop();
        let p = check(&b, &task()).join("\n");
        for needle in ["`oat` is not bound", "range", "can only be shown"] {
            assert!(p.contains(needle), "missing {needle} in\n{p}");
        }
        let mut shared = binding();
        shared.outputs[0].point = "floor1_TSupAirSet".into();
        assert!(check(&shared, &task()).join("\n").contains("every floor"));
    }
}
