//! Wiring and verification mistakes are refused before the run starts, instead of silently
//! holding or releasing points every tick.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use mm_graph::ParamValue;
use mm_runtime::{
    ConfigError, FailSafe, InputBinding, LoadError, RunningSequence, Scalar, Source, Supervisor,
};

fn load_err(bindings: Vec<InputBinding>) -> LoadError {
    let cxf = cxf();
    RunningSequence::load(SEQ, &cxf, &receipt(&cxf), bindings)
        .err()
        .unwrap()
}

#[test]
fn refuses_a_receipt_that_did_not_pass() {
    let cxf = cxf();
    let mut vectors = vectors();
    let mut text = serde_json::to_value(&vectors).unwrap();
    text["scenarios"][0]["expect"][0]["equals"] = serde_json::json!(0.0);
    vectors = serde_json::from_value(text).unwrap();
    let failing = mm_bridge::replay(&cxf, &vectors, &mm_bridge::ReplayLimits::default()).unwrap();
    assert!(!failing.passed());
    let err = RunningSequence::load(SEQ, &cxf, &failing, bindings())
        .err()
        .unwrap();
    assert_eq!(
        err,
        LoadError::NotPassed {
            sequence: SEQ.into()
        }
    );
}

#[test]
fn refuses_cxf_that_is_not_the_verified_model() {
    let verified = receipt(&cxf());
    let mut other = graph();
    // Any numeric parameter, nudged: a different model with the same interface.
    let param = other
        .blocks
        .iter_mut()
        .flat_map(|b| b.params.iter_mut())
        .find(|p| matches!(p.value, ParamValue::Number(_)))
        .unwrap();
    if let ParamValue::Number(x) = param.value {
        param.value = ParamValue::Number(x + 1.0);
    }
    let err = RunningSequence::load(SEQ, &emit(&other), &verified, bindings())
        .err()
        .unwrap();
    assert!(matches!(err, LoadError::NotVerified { .. }), "{err:?}");
}

#[test]
fn refuses_unbound_unknown_and_duplicate_inputs() {
    let mut short = bindings();
    short.pop();
    assert_eq!(
        load_err(short),
        LoadError::Unbound {
            sequence: SEQ.into(),
            input: "u1StaPro".into()
        }
    );

    let mut extra = bindings();
    extra.push(InputBinding {
        input: "nope".into(),
        source: Source::Constant {
            value: Scalar::Bool(true),
        },
    });
    assert!(matches!(load_err(extra), LoadError::UnknownInput { .. }));

    let mut twice = bindings();
    twice.push(bindings()[2].clone());
    assert!(matches!(
        load_err(twice),
        LoadError::DuplicateBinding { .. }
    ));
}

#[test]
fn refuses_sources_of_the_wrong_type() {
    let mut real_for_int = bindings();
    real_for_int[0].source = Source::Constant {
        value: Scalar::Real(0.0),
    };
    assert!(matches!(
        load_err(real_for_int),
        LoadError::BindingType {
            expected: "an Integer",
            ..
        }
    ));

    let mut reading_for_bool = bindings();
    reading_for_bool[1].source = Source::Measurement {
        point: "occ".into(),
    };
    assert!(matches!(
        load_err(reading_for_bool),
        LoadError::BindingType {
            expected: "a Boolean",
            given: "a Real",
            ..
        }
    ));
}

fn config_err(
    sequences: Vec<RunningSequence>,
    out: Vec<(String, mm_runtime::OutputBinding)>,
) -> ConfigError {
    Supervisor::new(sequences, out, FailSafe::HoldLast)
        .err()
        .unwrap()
}

#[test]
fn refuses_bad_bounds_instead_of_panicking_later() {
    for (min, max) in [(300.0, 280.0), (f64::NAN, 300.0), (0.0, f64::INFINITY)] {
        let err = config_err(
            vec![sequence(SEQ, bindings())],
            vec![binding("TSupSet", TSUP, min, max, None)],
        );
        assert!(
            matches!(err, ConfigError::BadBounds { .. }),
            "[{min}, {max}]: {err:?}"
        );
    }
}

#[test]
fn refuses_duplicate_points_and_sequences() {
    let out = vec![
        binding("TSupSet", TSUP, 0.0, 400.0, None),
        binding("dpSet", TSUP, 0.0, 1e6, None),
    ];
    assert!(matches!(
        config_err(vec![sequence(SEQ, bindings())], out),
        ConfigError::DuplicatePoint { .. }
    ));
    let twins = vec![sequence(SEQ, bindings()), sequence(SEQ, bindings())];
    assert!(matches!(
        config_err(twins, outputs(None, 288.15)),
        ConfigError::DuplicateSequence { .. }
    ));
}

#[test]
fn refuses_unknown_sequences_and_wrong_run_order() {
    let mut out = outputs(None, 288.15);
    out[0].0 = "ghost".into();
    assert!(matches!(
        config_err(vec![sequence(SEQ, bindings())], out),
        ConfigError::UnknownSequence { .. }
    ));

    let mut reads_later = bindings();
    reads_later[2].source = Source::Output {
        sequence: "later".into(),
        output: "y".into(),
    };
    let order = vec![sequence(SEQ, reads_later), sequence("later", bindings())];
    assert!(matches!(
        config_err(order, outputs(None, 288.15)),
        ConfigError::RunOrder { .. }
    ));

    let mut reads_itself = bindings();
    reads_itself[2].source = Source::Output {
        sequence: SEQ.into(),
        output: "y".into(),
    };
    let err = config_err(vec![sequence(SEQ, reads_itself)], outputs(None, 288.15));
    assert!(matches!(err, ConfigError::RunOrder { .. }), "{err:?}");
}

#[test]
fn refuses_a_gate_that_is_not_boolean() {
    let gate = Some(Source::Measurement {
        point: "occ".into(),
    });
    let err = config_err(vec![sequence(SEQ, bindings())], outputs(gate, 288.15));
    assert!(matches!(
        err,
        ConfigError::GateType {
            given: "a Real",
            ..
        }
    ));
}
