//! The home's demand-response figure: how much power the house drew during the run's most
//! expensive quarter of 15-minute prices, from the readings each tick recorded. Computed once at
//! the end of a home run and published as its own event, so a baseline run of the same window
//! (same prices, so the same quarter) can be set beside it. Ours, not a BOPTEST KPI.

use mm_runtime::home::{PRICE, forecast_key};
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
}
