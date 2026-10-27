//! The point map: which BACnet object mirrors which point. Instance numbers are the 1-based rank
//! of the point's name among the points of the same object type, so the same point list always
//! gives the same objects.

use std::collections::BTreeMap;

use bacnet_types::enums::{EngineeringUnits, ObjectType};
use serde::Serialize;

/// Where a point's value comes from. All of them are read-only to BACnet peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// A BOPTEST reading (an Analog Input).
    Measured,
    /// A verified sequence's output, computed by the engine (a read-only Analog Value).
    Derived,
    /// A setpoint the runtime sent to BOPTEST (a read-only Analog Value).
    Commanded,
}

impl Source {
    pub fn object_type(self) -> ObjectType {
        match self {
            Source::Measured => ObjectType::ANALOG_INPUT,
            Source::Derived | Source::Commanded => ObjectType::ANALOG_VALUE,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Source::Measured => "BOPTEST reading",
            Source::Derived => "sequence output (derived)",
            Source::Commanded => "setpoint sent to BOPTEST",
        }
    }
}

/// One point to mirror. `unit` is the BOPTEST unit string (`K`, `Pa`, `W`, `kg/s`, `1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointSpec {
    pub name: String,
    pub source: Source,
    pub unit: String,
}

/// A point with its BACnet object.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MappedPoint {
    pub name: String,
    pub source: Source,
    /// `analog-input` or `analog-value`.
    pub object_type: &'static str,
    pub instance: u32,
    pub unit: String,
    pub description: String,
    #[serde(skip)]
    pub units: EngineeringUnits,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MapError {
    #[error("point {0} is listed twice")]
    Duplicate(String),
    #[error("point names must not be empty")]
    EmptyName,
}

/// The objects for a point list, sorted by object type then instance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PointMap {
    pub points: Vec<MappedPoint>,
}

impl PointMap {
    pub fn new(specs: Vec<PointSpec>) -> Result<Self, MapError> {
        let mut by_type: BTreeMap<(u32, &str), &PointSpec> = BTreeMap::new();
        for spec in &specs {
            if spec.name.is_empty() {
                return Err(MapError::EmptyName);
            }
            // Object names are unique per device, across types.
            if by_type.values().any(|s| s.name == spec.name) {
                return Err(MapError::Duplicate(spec.name.clone()));
            }
            by_type.insert((spec.source.object_type().to_raw(), &spec.name), spec);
        }
        let mut points = Vec::with_capacity(by_type.len());
        let mut rank: BTreeMap<u32, u32> = BTreeMap::new();
        for ((ty, _), spec) in by_type {
            let instance = rank.entry(ty).or_insert(0);
            *instance += 1;
            points.push(MappedPoint {
                name: spec.name.clone(),
                source: spec.source,
                object_type: match spec.source.object_type() {
                    ObjectType::ANALOG_INPUT => "analog-input",
                    _ => "analog-value",
                },
                instance: *instance,
                unit: spec.unit.clone(),
                description: spec.source.label().to_owned(),
                units: units(&spec.unit),
            });
        }
        Ok(Self { points })
    }

    pub fn get(&self, name: &str) -> Option<&MappedPoint> {
        self.points.iter().find(|p| p.name == name)
    }
}

/// BACnet engineering units for a BOPTEST unit string; anything else is unitless.
pub fn units(unit: &str) -> EngineeringUnits {
    match unit {
        "K" => EngineeringUnits::DEGREES_KELVIN,
        "Pa" => EngineeringUnits::PASCALS,
        "W" => EngineeringUnits::WATTS,
        "kg/s" => EngineeringUnits::KILOGRAMS_PER_SECOND,
        "ppm" => EngineeringUnits::PARTS_PER_MILLION,
        _ => EngineeringUnits::NO_UNITS,
    }
}

/// The unit BOPTEST gives a point in `multizone_office_complex_air` (`docs/boptest-points.csv`),
/// with a trailing `_u`/`_y` name taken as is.
pub fn boptest_unit(name: &str) -> Option<&'static str> {
    const POINTS: &str = include_str!("../../../docs/boptest-points.csv");
    POINTS.lines().skip(1).find_map(|line| {
        let mut cols = line.splitn(4, ',');
        let point = cols.next()?;
        let _kind = cols.next()?;
        let unit = cols.next()?;
        (point == name).then_some(unit)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn spec(name: &str, source: Source) -> PointSpec {
        PointSpec {
            name: name.into(),
            source,
            unit: "K".into(),
        }
    }

    #[test]
    fn instances_rank_names_within_each_type() {
        let map = PointMap::new(vec![
            spec("zeta_y", Source::Measured),
            spec("chw.TSupSet", Source::Derived),
            spec("alpha_y", Source::Measured),
            spec("oveChiWatSys_TWSet_u", Source::Commanded),
        ])
        .unwrap();
        let got: Vec<_> = map
            .points
            .iter()
            .map(|p| (p.object_type, p.instance, p.name.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("analog-input", 1, "alpha_y"),
                ("analog-input", 2, "zeta_y"),
                ("analog-value", 1, "chw.TSupSet"),
                ("analog-value", 2, "oveChiWatSys_TWSet_u"),
            ]
        );
    }

    #[test]
    fn the_same_points_in_any_order_give_the_same_objects() {
        let a = vec![
            spec("b", Source::Measured),
            spec("a", Source::Derived),
            spec("c", Source::Measured),
        ];
        let mut b = a.clone();
        b.reverse();
        assert_eq!(PointMap::new(a).unwrap(), PointMap::new(b).unwrap());
    }

    #[test]
    fn duplicate_names_are_refused_across_types() {
        let err = PointMap::new(vec![
            spec("x", Source::Measured),
            spec("x", Source::Derived),
        ]);
        assert_eq!(err, Err(MapError::Duplicate("x".into())));
    }

    #[test]
    fn units_come_from_the_boptest_point_list() {
        assert_eq!(boptest_unit("floor1_reaAHU_TSup_y"), Some("K"));
        assert_eq!(boptest_unit("oveChiWatSys_dpSet_u"), Some("Pa"));
        assert_eq!(boptest_unit("floor1_reaAHU_occ_y"), Some("1"));
        assert_eq!(boptest_unit("not_a_point"), None);
        assert_eq!(units("K"), EngineeringUnits::DEGREES_KELVIN);
        assert_eq!(units("1"), EngineeringUnits::NO_UNITS);
    }
}
