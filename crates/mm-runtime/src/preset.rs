//! The demo wiring on BOPTEST `multizone_office_complex_air`: three air handler request
//! sequences (`specs/ahu_chw_requests`, one per floor) feeding the chilled-water plant reset
//! (`specs/chw_plant_reset`). Pure data: callers load each sequence from verified CXF with its
//! receipt, then hand these bindings to [`crate::RunningSequence::load`] and
//! [`crate::Supervisor::new`]. The live test and `mm-server` share it so they cannot drift.

use crate::wiring::{InputBinding, OutputBinding, Scalar, Source};

pub const CASE: &str = "multizone_office_complex_air";
/// Control step (s): the 120 s sample period of the request and trim-and-respond blocks.
pub const STEP_S: f64 = 120.0;
/// The air handlers, one per modelled floor.
pub const FLOORS: [u8; 3] = [1, 2, 3];
/// Spec folder of the request sequence and of the plant reset.
pub const REQUESTS_SPEC: &str = "ahu_chw_requests";
pub const RESET_SPEC: &str = "chw_plant_reset";
/// Sequence name of the plant reset in a running supervisor.
pub const RESET: &str = "chw";
/// BOPTEST points the plant reset writes (base names, without `_u`).
pub const TSUP_POINT: &str = "oveChiWatSys_TWSet";
pub const DP_POINT: &str = "oveChiWatSys_dpSet";

/// Sequence name of one floor's request instance.
pub fn requests_name(floor: u8) -> String {
    format!("ahu{floor}")
}

/// One floor's request inputs: its supply air temperature, the supply air setpoint in effect
/// (BOPTEST echoes every `_u` value, baseline or overwritten), and its measured valve position.
pub fn request_bindings(floor: u8) -> Vec<InputBinding> {
    let reading = |input: &str, point: String| InputBinding {
        input: input.into(),
        source: Source::Measurement { point },
    };
    vec![
        reading("TAirSup", format!("floor{floor}_reaAHU_TSup_y")),
        reading("TAirSupSet", format!("floor{floor}_TSupAirSet_u")),
        reading("uCooCoi", format!("floor{floor}_reaAHU_yCooVal_y")),
    ]
}

/// The plant reset's inputs. With `requests`, the request count is the sum of the floors'
/// request outputs (those sequences must run first); otherwise it is a constant 0. The plant
/// is enabled while any floor is occupied; staging is not observable on BOPTEST, so false.
pub fn reset_bindings(requests: bool) -> Vec<InputBinding> {
    let occupied = |floor: u8| Source::MeasurementAbove {
        point: format!("floor{floor}_reaAHU_occ_y"),
        threshold: 0.5,
    };
    let n_req = if requests {
        Source::SumInt {
            of: FLOORS
                .iter()
                .map(|f| Source::Output {
                    sequence: requests_name(*f),
                    output: "yChiWatResReq".into(),
                })
                .collect(),
        }
    } else {
        Source::Constant {
            value: Scalar::Int(0),
        }
    };
    vec![
        InputBinding {
            input: "nReqRes".into(),
            source: n_req,
        },
        InputBinding {
            input: "u1Ena".into(),
            source: Source::AnyTrue {
                of: FLOORS.iter().map(|f| occupied(*f)).collect(),
            },
        },
        InputBinding {
            input: "u1StaPro".into(),
            source: Source::Constant {
                value: Scalar::Bool(false),
            },
        },
    ]
}

/// The reset's two outputs, clamped to BOPTEST's supply temperature range and to the pump
/// design head (BOPTEST's own 19.13 MPa bound is a placeholder).
pub fn reset_outputs() -> Vec<(String, OutputBinding)> {
    let bind = |output: &str, point: &str, min: f64, max: f64| {
        (
            RESET.to_owned(),
            OutputBinding {
                output: output.into(),
                point: point.into(),
                min,
                max,
                gate: None,
            },
        )
    };
    vec![
        bind("TSupSet", TSUP_POINT, 278.15, 288.15),
        bind("dpSet", DP_POINT, 0.0, 478_250.0),
    ]
}

/// Readings worth showing beside the setpoints: plant response, power and the air handlers.
pub fn watch_points() -> Vec<String> {
    let mut points: Vec<String> = [
        "reaChiWatSys_TCHWSup_y",
        "reaChiWatSys_dp_y",
        "reaChiWatSys_mCHWTot_y",
        "reaChiWatSys_reaPChi_y",
        "reaChiWatSys_reaPPum_y",
        "reaChiWatSys_reaPCooTow_y",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for f in FLOORS {
        for p in [
            "reaAHU_TSup_y",
            "reaAHU_yCooVal_y",
            "reaAHU_occ_y",
            "TSupAirSet_u",
        ] {
            points.push(format!("floor{f}_{p}"));
        }
    }
    points
}
