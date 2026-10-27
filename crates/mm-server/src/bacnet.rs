//! The read-only BACnet mirror of live runs: which points it carries, and the values one tick
//! gives them. Replays never update it (they are not live data).

use std::collections::BTreeMap;

use mm_bacnet::{PointMap, PointSpec, Source, boptest_unit};
use mm_runtime::preset;
use serde::Serialize;
use ts_rs::TS;

use crate::run_dto::RunEvent;
use crate::specs::SpecStore;

/// The mirror's device and objects, for the web app.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BacnetInfo {
    pub device_instance: u32,
    pub port: u16,
    pub points: Vec<BacnetPoint>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BacnetPoint {
    pub name: String,
    /// `measured` (BOPTEST reading), `derived` (sequence output) or `commanded` (sent to
    /// BOPTEST).
    pub source: String,
    /// `analog-input` or `analog-value`.
    pub object_type: String,
    pub instance: u32,
    pub unit: String,
}

pub fn info(map: &PointMap, device_instance: u32, port: u16) -> BacnetInfo {
    BacnetInfo {
        device_instance,
        port,
        points: map
            .points
            .iter()
            .map(|p| BacnetPoint {
                name: p.name.clone(),
                source: match p.source {
                    Source::Measured => "measured",
                    Source::Derived => "derived",
                    Source::Commanded => "commanded",
                }
                .into(),
                object_type: p.object_type.into(),
                instance: p.instance,
                unit: p.unit.clone(),
            })
            .collect(),
    }
}

/// The points a live run produces: the watched BOPTEST readings, every sequence output
/// (`<instance>.<output>`), and the setpoints the runtime sends (two plant setpoints and, with
/// the pressure resets, one per floor). Points a run does not produce read as no data.
pub fn point_map(specs: &SpecStore) -> Result<PointMap, mm_bacnet::MapError> {
    // The static pressure setpoints BOPTEST echoes back are listed once, as sent (below).
    let mut points: Vec<PointSpec> = preset::watch_points()
        .into_iter()
        .chain(
            preset::pressure_watch_points()
                .into_iter()
                .filter(|p| !p.ends_with("_u")),
        )
        .map(|name| PointSpec {
            unit: boptest_unit(&name).unwrap_or("1").to_owned(),
            name,
            source: Source::Measured,
        })
        .collect();
    let mut instances: Vec<(String, &str)> = preset::FLOORS
        .iter()
        .map(|f| (preset::requests_name(*f), preset::REQUESTS_SPEC))
        .collect();
    instances.push((preset::RESET.to_owned(), preset::RESET_SPEC));
    for f in preset::FLOORS {
        for z in preset::ZONES {
            instances.push((preset::zone_name(f, z), preset::ZONE_PRESSURE_SPEC));
        }
        instances.push((preset::pressure_name(f), preset::PRESSURE_SPEC));
    }
    for (instance, spec) in instances {
        let Some(spec) = specs.get(spec) else {
            continue;
        };
        for out in &spec.task.outputs {
            points.push(PointSpec {
                name: format!("{instance}.{}", out.name),
                source: Source::Derived,
                unit: out.unit.clone(),
            });
        }
    }
    let floor_points = preset::pressure_outputs().into_iter().map(|(_, o)| o.point);
    for point in [preset::TSUP_POINT.to_owned(), preset::DP_POINT.to_owned()]
        .into_iter()
        .chain(floor_points)
    {
        let name = format!("{point}_u");
        points.push(PointSpec {
            unit: boptest_unit(&name).unwrap_or("1").to_owned(),
            name,
            source: Source::Commanded,
        });
    }
    PointMap::new(points)
}

/// One tick's values by point name. Booleans read as 0 or 1; an enumeration as its ordinal.
pub fn tick_values(event: &RunEvent) -> BTreeMap<String, f64> {
    let RunEvent::Tick {
        outputs,
        writes,
        readings,
        ..
    } = event
    else {
        return BTreeMap::new();
    };
    let mut values = readings.clone();
    values.extend(
        writes
            .iter()
            .filter(|(k, _)| k.ends_with("_u"))
            .map(|(k, v)| (k.clone(), *v)),
    );
    if let Some(instances) = outputs.as_object() {
        for (instance, outs) in instances {
            let Some(outs) = outs.as_object() else {
                continue;
            };
            for (name, value) in outs {
                let number = match value {
                    serde_json::Value::Number(n) => n.as_f64(),
                    serde_json::Value::Bool(b) => Some(f64::from(u8::from(*b))),
                    serde_json::Value::Object(o) => o.get("ordinal").and_then(|v| v.as_f64()),
                    _ => None,
                };
                if let Some(number) = number {
                    values.insert(format!("{instance}.{name}"), number);
                }
            }
        }
    }
    values
}

/// A started mirror and its description.
pub struct BacnetMirror {
    pub mirror: mm_bacnet::Mirror,
    pub info: BacnetInfo,
}

/// A mirror update may not hold up the control loop for long: BACnet is a view, not the
/// control path.
const UPDATE_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Updates the mirror from one live tick; problems are logged, never fatal to the run.
pub async fn update(bacnet: &BacnetMirror, event: &RunEvent) {
    let values = tick_values(event);
    match tokio::time::timeout(UPDATE_BUDGET, bacnet.mirror.update(&values)).await {
        Ok(report) if !report.errors.is_empty() => {
            tracing::warn!(errors = ?report.errors, "BACnet mirror refused some points");
        }
        Ok(_) => {}
        Err(_) => tracing::warn!("BACnet mirror update took too long; skipped this tick"),
    }
}

/// Marks every point no data (a run ended).
pub async fn clear(bacnet: &BacnetMirror) {
    if tokio::time::timeout(UPDATE_BUDGET, bacnet.mirror.mark_all_no_data())
        .await
        .is_err()
    {
        tracing::warn!("BACnet mirror clear took too long");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_gives_readings_outputs_and_sent_setpoints() {
        let event = RunEvent::Tick {
            outputs: serde_json::json!({
                "chw": {"TSupSet": 278.71, "dpSet": "NaN"},
                "ahu1": {"yChiPlaReq": true, "yChiWatResReq": 2}
            }),
            writes: BTreeMap::from([
                ("oveChiWatSys_TWSet_u".to_owned(), 278.71),
                ("oveChiWatSys_TWSet_activate".to_owned(), 1.0),
            ]),
            readings: BTreeMap::from([("reaChiWatSys_dp_y".to_owned(), 41_000.0)]),
            events: Vec::new(),
        };
        let values = tick_values(&event);
        assert_eq!(
            values,
            BTreeMap::from([
                ("ahu1.yChiPlaReq".to_owned(), 1.0),
                ("ahu1.yChiWatResReq".to_owned(), 2.0),
                ("chw.TSupSet".to_owned(), 278.71),
                ("oveChiWatSys_TWSet_u".to_owned(), 278.71),
                ("reaChiWatSys_dp_y".to_owned(), 41_000.0),
            ])
        );
    }
}
