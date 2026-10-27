//! Checks on bindings of engineers' sequences to BOPTEST (`mm_server::bind`).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use mm_agent::TaskSpec;
use mm_runtime::Source;
use mm_server::bind::{Binding, POINTS, check, on, watch, with_units};
use serde_json::json;

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

fn refused(b: &Binding) -> String {
    let p = check(b, &task()).join("\n");
    assert!(!p.is_empty(), "accepted:\n{b:?}");
    p
}

#[test]
fn each_kind_of_bad_binding_is_refused_with_its_reason() {
    let mut b = binding();
    b.inputs[1].point = "floor{floor}_nope_y".into();
    assert!(refused(&b).contains("not a BOPTEST measurement"));

    let mut b = binding();
    b.inputs.pop();
    assert!(refused(&b).contains("`oat` is not bound"));

    let mut b = binding();
    b.inputs[0].kind = "measurement".into();
    assert!(refused(&b).contains("cannot be fed as `measurement`"));

    let mut b = binding();
    b.inputs[1] = serde_json::from_value(
        json!({"input": "oat", "kind": "constant", "point": "", "threshold": 0, "value": true}),
    )
    .unwrap();
    assert!(refused(&b).contains("constant of its type"));
    b.inputs[1].value = json!(290.0);
    assert!(
        check(&b, &task()).is_empty(),
        "a real constant feeds a real input"
    );

    let mut b = binding();
    b.inputs.push(b.inputs[1].clone());
    assert!(refused(&b).contains("bound twice"));

    let mut b = binding();
    b.inputs[1].point = "floor{floor}_reaAHU_phiAHURet_y".into();
    assert!(
        refused(&b).contains("is in K but"),
        "a fraction does not feed a kelvin input"
    );

    let mut b = binding();
    b.outputs[0].max = 400.0;
    assert!(refused(&b).contains("range"));

    let mut b = binding();
    b.outputs[0].measured = "floor{floor}_nope_y".into();
    assert!(refused(&b).contains("is not a BOPTEST measurement"));

    let mut b = binding();
    b.outputs[1].point = "floor{floor}_TSupAirSet".into();
    assert!(refused(&b).contains("can only be shown"));

    let mut b = binding();
    b.outputs[0].point = "floor1_TSupAirSet".into();
    assert!(refused(&b).contains("every floor"));

    let mut b = binding();
    b.outputs[0].point = "floor{floor}_duaFanAirHanUni_supFan_oveSpeSupFan".into();
    b.outputs[0].min = 0.0;
    b.outputs[0].max = 1.0;
    assert!(
        refused(&b).contains("not a supervisory setpoint"),
        "fan speed is an actuator"
    );

    let mut b = binding();
    b.floors = vec![1, 1];
    assert!(refused(&b).contains("each once"));
}

#[test]
fn units_come_from_the_point_list() {
    let b = with_units(binding());
    assert_eq!(b.units["floor{floor}_TSupAirSet_u"], "K");
    assert_eq!(b.units["floor{floor}_reaAHU_occ_y"], "1");
    assert_eq!(b.units["weaSta_reaWeaTDryBul_y"], "K");
}
