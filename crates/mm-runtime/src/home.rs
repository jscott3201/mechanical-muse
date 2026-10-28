//! The home wiring on BOPTEST `bestest_hydronic_heat_pump` (a simulated house in Brussels with an
//! air-to-water heat pump and floor heating): one instance of `specs/home_price_response` writes
//! the zone setpoint. Pure data, like [`crate::preset`].
//!
//! The sequence looks ahead, so it reads BOPTEST's forecast as well as its measurements. The
//! runtime makes no decision with it: each step it asks BOPTEST for the forecast and passes the
//! values at whole hours ahead in as plain readings under [`forecast_key`] names. Everything
//! done with them happens in the verified graph.

use std::collections::BTreeMap;

use crate::plant::Measurements;
use crate::wiring::{InputBinding, OutputBinding, Source};

pub const CASE: &str = "bestest_hydronic_heat_pump";
/// Control step (s): the 15-minute step the sequence was tuned with.
pub const STEP_S: f64 = 900.0;
/// BOPTEST's electricity price scenario the home runs under (spot prices).
pub const PRICE_SCENARIO: &str = "highly_dynamic";
/// Spec folder and running name of the sequence.
pub const SPEC: &str = "home_price_response";
pub const NAME: &str = "home";
/// The BOPTEST point the sequence writes (base name, without `_u`).
pub const TSET_POINT: &str = "oveTSet";
/// Forecast variables: the spot price and the lower and upper comfort limits.
pub const PRICE: &str = "PriceElectricPowerHighlyDynamic";
pub const FLOOR: &str = "LowerSetp[1]";
pub const CEILING: &str = "UpperSetp[1]";
/// Hours of each forecast the sequence reads (from now): 24 prices, 3 floors, 4 ceilings.
const PRICE_HOURS: u32 = 24;
const FLOOR_HOURS: u32 = 3;
const CEILING_HOURS: u32 = 4;
// One forecast request covers all three, so the prices must be the longest look-ahead.
const _: () = assert!(FLOOR_HOURS <= PRICE_HOURS && CEILING_HOURS <= PRICE_HOURS);
/// Setpoint limits (K): 15 to 27 degC, inside BOPTEST's 5 to 35 degC and around every value the
/// sequence can send in this building's comfort band.
const TSET_MIN: f64 = 288.15;
const TSET_MAX: f64 = 300.15;

/// The reading name for `point` forecast `hours` ahead (0 = now). The one place this is spelled.
pub fn forecast_key(point: &str, hours: u32) -> String {
    format!("forecast:{point}:+{hours}h")
}

/// What to ask BOPTEST's `/forecast` for each step: the points, the horizon and the interval
/// (s). Hourly values from now through 23 h ahead.
pub fn forecast_request() -> (Vec<&'static str>, f64, f64) {
    (
        vec![PRICE, FLOOR, CEILING],
        f64::from(PRICE_HOURS - 1) * 3600.0,
        3600.0,
    )
}

/// Adds a forecast reply to the readings: the series of `point` at index `h` becomes
/// [`forecast_key`]`(point, h)`. Values that are not finite are left out, so the sequence that
/// needs them degrades visibly instead of guessing.
pub fn add_forecast(readings: &mut Measurements, forecast: &BTreeMap<String, Vec<f64>>) {
    for point in [PRICE, FLOOR, CEILING] {
        let Some(series) = forecast.get(point) else {
            continue;
        };
        for (h, value) in (0u32..).zip(series) {
            if value.is_finite() {
                readings.insert(forecast_key(point, h), *value);
            }
        }
    }
}

/// The sequence's inputs: hourly prices, the coming floors and ceilings, the room temperature.
pub fn bindings() -> Vec<InputBinding> {
    let reading = |input: String, point: String| InputBinding {
        input,
        source: Source::Measurement { point },
    };
    let mut out: Vec<InputBinding> = (0..PRICE_HOURS)
        .map(|h| reading(format!("pri{h:02}"), forecast_key(PRICE, h)))
        .collect();
    out.extend((0..FLOOR_HOURS).map(|h| reading(format!("TLo{h:02}"), forecast_key(FLOOR, h))));
    out.extend((0..CEILING_HOURS).map(|h| reading(format!("THi{h:02}"), forecast_key(CEILING, h))));
    out.push(reading("TZon".into(), "reaTZon_y".into()));
    out
}

/// The one output: the zone setpoint, always written (the house has no occupancy gate of its
/// own; the comfort limits already follow occupancy).
pub fn outputs() -> Vec<(String, OutputBinding)> {
    vec![(
        NAME.to_owned(),
        OutputBinding {
            output: "TSet".into(),
            point: TSET_POINT.into(),
            min: TSET_MIN,
            max: TSET_MAX,
            gate: None,
        },
    )]
}

/// Readings worth showing and recording: the room, the setpoints BOPTEST reports, the power the
/// heat pump, its fan and the floor pump draw, the outdoor air, and the price and comfort band
/// now. Nothing here is read to decide anything except through [`bindings`].
pub fn watch_points() -> Vec<String> {
    let mut points: Vec<String> = [
        "reaTZon_y",
        "reaTSetHea_y",
        "reaPHeaPum_y",
        "reaPFan_y",
        "reaPPumEmi_y",
        "weaSta_reaWeaTDryBul_y",
        "oveTSet_u",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    points.extend([PRICE, FLOOR, CEILING].map(|p| forecast_key(p, 0)));
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_cover_the_task_inputs_in_order() {
        let names: Vec<String> = bindings().into_iter().map(|b| b.input).collect();
        assert_eq!(names.len(), 32);
        assert_eq!(names[0], "pri00");
        assert_eq!(names[23], "pri23");
        assert_eq!(names[24..27], ["TLo00", "TLo01", "TLo02"]);
        assert_eq!(names[27..31], ["THi00", "THi01", "THi02", "THi03"]);
        assert_eq!(names[31], "TZon");
    }

    #[test]
    fn forecast_values_land_under_their_hours() {
        let mut readings = Measurements::new();
        let forecast = BTreeMap::from([
            (PRICE.to_owned(), vec![0.2, 0.3, f64::NAN]),
            ("time".to_owned(), vec![0.0, 3600.0, 7200.0]),
        ]);
        add_forecast(&mut readings, &forecast);
        assert_eq!(readings.get(&forecast_key(PRICE, 0)), Some(&0.2));
        assert_eq!(readings.get(&forecast_key(PRICE, 1)), Some(&0.3));
        // Not finite: left out, so the sequence degrades rather than reading NaN.
        assert!(!readings.contains_key(&forecast_key(PRICE, 2)));
        assert_eq!(readings.len(), 2);
    }

    #[test]
    fn forecast_covers_every_hour_read() {
        let (points, horizon, interval) = forecast_request();
        assert_eq!(points.len(), 3);
        // Values at 0..=horizon/interval hours: 24, enough for the prices (the longest).
        assert_eq!(horizon / interval + 1.0, f64::from(PRICE_HOURS));
    }
}
