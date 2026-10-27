//! The demo wiring on BOPTEST `multizone_office_complex_air`: three air handler request
//! sequences (`specs/ahu_chw_requests`, one per floor) feeding the chilled-water plant reset
//! (`specs/chw_plant_reset`), and optionally a duct static pressure reset per floor
//! (`specs/ahu_static_pressure_reset`) fed by its five zones' requests
//! (`specs/zone_pressure_requests`). Pure data: callers load each sequence from verified CXF with its
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

/// Spec folders of the zone pressure requests and of the per-floor static pressure reset.
pub const ZONE_PRESSURE_SPEC: &str = "zone_pressure_requests";
pub const PRESSURE_SPEC: &str = "ahu_static_pressure_reset";
/// BOPTEST's zone names on every floor, in its own order (`vAV1` to `vAV5`).
pub const ZONES: [&str; 5] = ["Cor", "Sou", "Eas", "Nor", "Wes"];
/// Supply airflow (m3/s) above which a floor's supply fan counts as running.
pub const FAN_ON_FLOW: f64 = 0.1;

/// Sequence name of one zone's pressure request instance, e.g. `zone2eas`.
pub fn zone_name(floor: u8, zone: &str) -> String {
    format!("zone{floor}{}", zone.to_ascii_lowercase())
}

/// Sequence name of one floor's static pressure reset, e.g. `sp2`.
pub fn pressure_name(floor: u8) -> String {
    format!("sp{floor}")
}

/// One zone's request inputs: its damper position, airflow and airflow setpoint.
pub fn zone_bindings(floor: u8, zone: &str) -> Vec<InputBinding> {
    let reading = |input: &str, point: String| InputBinding {
        input: input.into(),
        source: Source::Measurement { point },
    };
    vec![
        reading("uDam", format!("floor{floor}_reaZon{zone}_yDam_y")),
        reading("VDis_flow", format!("floor{floor}_reaZon{zone}_V_flow_y")),
        reading(
            "VSet_flow",
            format!("floor{floor}_reaZon{zone}_V_flowSet_y"),
        ),
    ]
}

/// One floor's reset inputs: the sum of its zones' requests (those sequences must run first),
/// and the fan counted as running while the floor's supply airflow is above [`FAN_ON_FLOW`].
pub fn pressure_bindings(floor: u8) -> Vec<InputBinding> {
    vec![
        InputBinding {
            input: "nReqPre".into(),
            source: Source::SumInt {
                of: ZONES
                    .iter()
                    .map(|z| Source::Output {
                        sequence: zone_name(floor, z),
                        output: "yPreReq".into(),
                    })
                    .collect(),
            },
        },
        InputBinding {
            input: "u1FanOn".into(),
            source: Source::MeasurementAbove {
                point: format!("floor{floor}_reaAHU_V_flowSup_y"),
                threshold: FAN_ON_FLOW,
            },
        },
    ]
}

/// Each floor's static pressure setpoint, clamped to BOPTEST's 50 Pa lower bound and to its own
/// constant 400 Pa (its overwrite allows up to 410 Pa).
pub fn pressure_outputs() -> Vec<(String, OutputBinding)> {
    FLOORS
        .iter()
        .map(|f| {
            (
                pressure_name(*f),
                OutputBinding {
                    output: "dpSet".into(),
                    point: format!("floor{f}_dpSet"),
                    min: 50.0,
                    max: 400.0,
                    gate: None,
                },
            )
        })
        .collect()
}

/// Readings worth showing beside the static pressure setpoints: each floor's duct static
/// pressure and supply fan power.
pub fn pressure_watch_points() -> Vec<String> {
    FLOORS
        .iter()
        .flat_map(|f| {
            [
                format!("floor{f}_reaAHU_dp_y"),
                format!("floor{f}_reaAHU_PFanSup_y"),
                format!("floor{f}_dpSet_u"),
            ]
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod pressure_tests {
    use super::*;

    #[test]
    fn every_bound_point_is_a_boptest_point() {
        let points = include_str!("../../../docs/boptest-points.csv");
        let known = |p: &str| points.lines().any(|l| l.split(',').next() == Some(p));
        let mut bound = Vec::new();
        for f in FLOORS {
            for z in ZONES {
                for b in zone_bindings(f, z) {
                    if let Source::Measurement { point } = b.source {
                        bound.push(point);
                    }
                }
            }
            for b in pressure_bindings(f) {
                if let Source::MeasurementAbove { point, .. } = b.source {
                    bound.push(point);
                }
            }
        }
        bound.extend(pressure_watch_points());
        bound.extend(
            pressure_outputs()
                .into_iter()
                .map(|(_, o)| format!("{}_u", o.point)),
        );
        assert_eq!(bound.len(), 15 * 3 + 3 + 9 + 3);
        for p in &bound {
            assert!(known(p), "{p} is not a BOPTEST point");
        }
    }

    #[test]
    fn each_floor_sums_its_own_five_zones() {
        let b = pressure_bindings(2);
        let Source::SumInt { of } = &b[0].source else {
            panic!("nReqPre must be a sum")
        };
        let names: Vec<_> = of
            .iter()
            .map(|s| match s {
                Source::Output { sequence, .. } => sequence.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(
            names,
            ["zone2cor", "zone2sou", "zone2eas", "zone2nor", "zone2wes"]
        );
    }
}
