//! The home's demand-response figure: how much power the house drew during the run's most
//! expensive quarter of 15-minute prices, from the readings each tick recorded. Computed once at
//! the end of a home run and published as its own event, so a baseline run of the same window
//! (same prices, so the same quarter) can be set beside it. Ours, not a BOPTEST KPI.

use mm_runtime::home::{FLOOR, PRICE, forecast_key};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::run_dto::{Envelope, RunEvent};

/// The share of steps, by price, counted as the expensive ones.
const TOP_SHARE: f64 = 0.25;
/// The house's electric power: heat pump, its fan and the floor heating pump (W).
const POWER: [&str; 3] = ["reaPHeaPum_y", "reaPFan_y", "reaPPumEmi_y"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PeakPower {
    /// Price at or above which a step is among the most expensive quarter (per kWh).
    pub threshold: f64,
    /// Mean electric power in those steps (kW).
    pub peak_kw: f64,
    /// Mean electric power over all steps (kW).
    pub mean_kw: f64,
    pub peak_steps: u32,
    pub steps: u32,
}

/// The figure for a home run's events, or `None` when no tick has a price and all three powers.
pub fn peak_power(events: &[Envelope]) -> Option<PeakPower> {
    let price_key = forecast_key(PRICE, 0);
    let samples: Vec<(f64, f64)> = events
        .iter()
        .filter_map(|e| match &e.event {
            RunEvent::Tick { readings, .. } => {
                let price = *readings.get(&price_key)?;
                let watts = POWER
                    .iter()
                    .map(|p| readings.get(*p).copied())
                    .sum::<Option<f64>>()?;
                Some((price, watts / 1000.0))
            }
            _ => None,
        })
        .collect();
    if samples.is_empty() {
        return None;
    }
    let mut prices: Vec<f64> = samples.iter().map(|s| s.0).collect();
    prices.sort_by(f64::total_cmp);
    // The value at the 75th percentile position (as the probe in docs/results-log.md counted it).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = ((prices.len() as f64) * (1.0 - TOP_SHARE)) as usize;
    let threshold = *prices.get(at.min(prices.len() - 1))?;
    let peak: Vec<f64> = samples
        .iter()
        .filter(|s| s.0 >= threshold)
        .map(|s| s.1)
        .collect();
    let mean = |v: &[f64]| {
        #[allow(clippy::cast_precision_loss)]
        let n = v.len() as f64;
        v.iter().sum::<f64>() / n
    };
    let all: Vec<f64> = samples.iter().map(|s| s.1).collect();
    Some(PeakPower {
        threshold,
        peak_kw: mean(&peak),
        mean_kw: mean(&all),
        peak_steps: u32::try_from(peak.len()).unwrap_or(u32::MAX),
        steps: u32::try_from(samples.len()).unwrap_or(u32::MAX),
    })
}

/// What the house's setpoint did, step by step, judged against the plain rule the sequence
/// starts from ("normal": 0.2 K above the comfort floor now, never below 20.5 degC). Coming
/// floors are not in the record, so a rise before people come home counts as preheating too.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Behavior {
    pub preheat_hours: f64,
    pub coast_hours: f64,
    pub normal_hours: f64,
    /// Mean price now (per kWh) over the steps in each mode; `None` without such steps.
    pub preheat_mean_price: Option<f64>,
    pub coast_mean_price: Option<f64>,
    pub normal_mean_price: Option<f64>,
}

/// Normal's floor, 20.5 degC, and its margin above the comfort floor (spec.md parameters).
const AWAY_K: f64 = 293.65;
const MARGIN_K: f64 = 0.2;
/// A setpoint this far from normal counts as preheating or coasting.
const BAND_K: f64 = 0.05;

/// The modes of a home run from its ticks (setpoint sent, comfort floor now, price now), or
/// `None` when no tick has all three (a baseline sends no setpoint).
pub fn behavior(events: &[Envelope], step_s: f64) -> Option<Behavior> {
    let floor_key = forecast_key(FLOOR, 0);
    let price_key = forecast_key(PRICE, 0);
    let mut sums = [(0u32, 0.0f64); 3]; // preheat, coast, normal: (steps, price sum)
    for e in events {
        let RunEvent::Tick {
            writes, readings, ..
        } = &e.event
        else {
            continue;
        };
        let (Some(sent), Some(floor), Some(price)) = (
            writes.get("oveTSet_u"),
            readings.get(&floor_key),
            readings.get(&price_key),
        ) else {
            continue;
        };
        if writes.get("oveTSet_activate") != Some(&1.0) {
            continue;
        }
        let normal = (floor + MARGIN_K).max(AWAY_K);
        let mode = if *sent > normal + BAND_K {
            0
        } else if *sent < normal - BAND_K {
            1
        } else {
            2
        };
        if let Some(slot) = sums.get_mut(mode) {
            slot.0 += 1;
            slot.1 += price;
        }
    }
    if sums.iter().all(|s| s.0 == 0) {
        return None;
    }
    let hours = |n: u32| f64::from(n) * step_s / 3600.0;
    let mean = |(n, sum): (u32, f64)| (n > 0).then(|| sum / f64::from(n));
    let [p, c, n] = sums;
    Some(Behavior {
        preheat_hours: hours(p.0),
        coast_hours: hours(c.0),
        normal_hours: hours(n.0),
        preheat_mean_price: mean(p),
        coast_mean_price: mean(c),
        normal_mean_price: mean(n),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn tick(price: f64, watts: f64) -> Envelope {
        let readings = BTreeMap::from([
            (forecast_key(PRICE, 0), price),
            ("reaPHeaPum_y".to_owned(), watts),
            ("reaPFan_y".to_owned(), 0.0),
            ("reaPPumEmi_y".to_owned(), 0.0),
        ]);
        Envelope {
            run_id: "r".into(),
            seq: 0,
            sim_time_s: Some(0.0),
            event: RunEvent::Tick {
                outputs: serde_json::Value::Null,
                writes: BTreeMap::new(),
                readings,
                events: Vec::new(),
            },
        }
    }

    #[test]
    fn the_expensive_quarter_is_averaged_apart() {
        // Eight steps: the two dearest (0.3) drew 1 kW, the rest 3 kW.
        let mut events: Vec<Envelope> = (0..6).map(|_| tick(0.2, 3000.0)).collect();
        events.push(tick(0.3, 1000.0));
        events.push(tick(0.3, 1000.0));
        let p = peak_power(&events).unwrap();
        assert_eq!(p.threshold, 0.3);
        assert_eq!(p.peak_steps, 2);
        assert!((p.peak_kw - 1.0).abs() < 1e-12);
        assert!((p.mean_kw - 2.5).abs() < 1e-12);
        assert_eq!(p.steps, 8);
    }

    #[test]
    fn ticks_without_the_readings_are_skipped() {
        let mut partial = tick(0.2, 1000.0);
        if let RunEvent::Tick { readings, .. } = &mut partial.event {
            readings.remove("reaPFan_y");
        }
        assert_eq!(peak_power(&[partial]), None);
        assert_eq!(peak_power(&[]), None);
    }

    #[test]
    fn modes_follow_the_setpoint_against_normal() {
        let tick = |sent: f64, floor: f64, price: f64| {
            let mut e = tick(price, 0.0);
            if let RunEvent::Tick {
                writes, readings, ..
            } = &mut e.event
            {
                writes.insert("oveTSet_u".into(), sent);
                writes.insert("oveTSet_activate".into(), 1.0);
                readings.insert(forecast_key(FLOOR, 0), floor);
            }
            e
        };
        // Occupied (floor 21 degC, normal 21.2): preheat 22.2, coast 21.1, normal 21.2;
        // away (floor 15, normal 20.5): normal 20.5.
        let events = vec![
            tick(295.35, 294.15, 0.20),
            tick(295.35, 294.15, 0.22),
            tick(294.25, 294.15, 0.30),
            tick(294.35, 294.15, 0.25),
            tick(293.65, 288.15, 0.25),
        ];
        let b = behavior(&events, 900.0).unwrap();
        assert_eq!(
            (b.preheat_hours, b.coast_hours, b.normal_hours),
            (0.5, 0.25, 0.5)
        );
        assert!((b.preheat_mean_price.unwrap() - 0.21).abs() < 1e-12);
        assert_eq!(b.coast_mean_price, Some(0.30));
        // A baseline sends no setpoint: no modes.
        assert_eq!(behavior(&[super::tests::tick(0.2, 1.0)], 900.0), None);
    }
}
