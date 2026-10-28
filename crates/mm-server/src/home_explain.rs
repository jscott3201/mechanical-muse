//! The facts a home run's explanation may quote: its energy, emissions and cost (all electric,
//! spot prices), its power in peak-price hours, how its setpoint behaved, and the same against
//! BOPTEST's controller over the same window. Rounded here, once, so the model only quotes
//! (`mm_agent::unsupported_numbers` refuses any other number).

use serde_json::{Value, json};

use crate::home_facts::{self, PeakPower};
use crate::ledger::{self, RunLedger};
use crate::routes::AppState;
use crate::run_dto::{Envelope, RunEvent};

fn round(x: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (x * f).round() / f
}

fn pct(change: f64, base: f64) -> Option<f64> {
    (base.abs() > f64::EPSILON).then(|| round(change / base.abs() * 100.0, 1))
}

/// BOPTEST's peak electric demand KPI (kW per m2) from a run's events.
fn pele(events: &[Envelope]) -> Option<f64> {
    events.iter().rev().find_map(|e| match &e.event {
        RunEvent::Kpi { values } => values.get("pele_tot").copied(),
        _ => None,
    })
}

fn peak(events: &[Envelope]) -> Option<PeakPower> {
    events.iter().rev().find_map(|e| match &e.event {
        RunEvent::PeakPower { peak } => Some(peak.clone()),
        _ => None,
    })
}

/// The events of another run by its ledger reference (`run:<id>` or `replay:<name>`).
async fn events_of(state: &AppState, reference: &str) -> Option<Vec<Envelope>> {
    if let Some(id) = reference.strip_prefix("run:") {
        return Some(state.runs().get(id)?.journal().since(None));
    }
    let name = reference.strip_prefix("replay:")?.to_owned();
    let dir = state.runs().config.replays.clone()?;
    tokio::task::spawn_blocking(move || crate::replay::read(&dir, &name).ok())
        .await
        .ok()
        .flatten()
}

fn totals(l: &RunLedger) -> Value {
    json!({
        "electricity_kwh": round(l.scope2.kwh, 1),
        "kg_co2e": round(l.scope2.kg_co2e, 1),
        "cost_eur": round(l.boptest_cost, 2),
        "average_price_eur_per_kwh": round(l.average_price_per_kwh, 4),
        "thermal_discomfort_kh": l.tdis_tot.map(|v| round(v, 2)),
    })
}

/// The facts for a home run whose ledger is `own` and events `events`; `others` are the runs
/// of the same window.
pub(crate) async fn facts(
    state: &AppState,
    own: &RunLedger,
    others: &[RunLedger],
    events: &[Envelope],
) -> Value {
    let area = own.factors.area_m2;
    let own_peak = peak(events);
    let baseline = if own.subject.baseline {
        None
    } else {
        others.iter().find(|l| l.subject.baseline)
    };
    let vs_baseline = match baseline {
        Some(b) => {
            let base_events = events_of(state, &b.subject.reference)
                .await
                .unwrap_or_default();
            let base_peak = peak(&base_events);
            let c = ledger::compare(b.clone(), own.clone()).ok();
            let d = c.as_ref().map(|c| &c.delta);
            Some(json!({
                "compared_with": b.subject.reference,
                "baseline": totals(b),
                "change_negative_is_less": d.map(|d| json!({
                    "electricity_kwh": round(d.total.kwh, 1),
                    "kg_co2e": round(d.total.kg_co2e, 1),
                    "cost_eur": round(d.total.cost, 2),
                    "kg_co2e_percent": pct(d.total.kg_co2e, b.total.kg_co2e),
                    "cost_percent": pct(d.total.cost, b.total.cost),
                    "thermal_discomfort_percent": match (own.tdis_tot, b.tdis_tot) {
                        (Some(o), Some(bb)) => pct(o - bb, bb),
                        _ => None,
                    },
                })),
                "peak_price_power_kw_baseline": base_peak.as_ref().map(|p| round(p.peak_kw, 2)),
                "peak_price_power_change_percent": match (&own_peak, &base_peak) {
                    (Some(o), Some(bp)) => pct(o.peak_kw - bp.peak_kw, bp.peak_kw),
                    _ => None,
                },
                "highest_15_min_draw_kw_baseline": pele(&base_events).map(|v| round(v * area, 2)),
                "highest_15_min_draw_change_percent": match (pele(events), pele(&base_events)) {
                    (Some(o), Some(bp)) => pct(o - bp, bp),
                    _ => None,
                },
            }))
        }
        None => None,
    };
    let behavior = home_facts::behavior(events, own.subject.step_s).map(|b| {
        json!({
            "preheat_hours": round(b.preheat_hours, 1),
            "coast_hours": round(b.coast_hours, 1),
            "normal_hours": round(b.normal_hours, 1),
            "mean_price_eur_per_kwh_while_preheating": b.preheat_mean_price.map(|v| round(v, 3)),
            "mean_price_eur_per_kwh_while_coasting": b.coast_mean_price.map(|v| round(v, 3)),
            "mean_price_eur_per_kwh_otherwise": b.normal_mean_price.map(|v| round(v, 3)),
        })
    });
    json!({
        "basis": "BOPTEST simulation of a heat pump house in Brussels on spot electricity prices in \
            euros; one emission factor all year (0.167 kgCO2e per kWh), so emissions follow energy \
            only; heating energy only (heat pump, its fan, floor pump)",
        "this_run": totals(own),
        "peak_price_power": own_peak.as_ref().map(|p| json!({
            "meaning": "mean electric power in the window's most expensive quarter of 15-minute prices",
            "kw": round(p.peak_kw, 2),
            "mean_kw_all_hours": round(p.mean_kw, 2),
            "price_threshold_eur_per_kwh": round(p.threshold, 3),
        })),
        "highest_15_min_draw_kw": pele(events).map(|v| round(v * area, 2)),
        "setpoint_behavior": behavior,
        "vs_baseline": vs_baseline,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::path::Path;

    use crate::runs::{PlantSource, RunsConfig};

    #[tokio::test]
    async fn a_home_run_gets_its_behavior_peak_power_and_baseline() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let palette = mm_graph::Palette::pinned().unwrap();
        let specs = crate::SpecStore::load(&root.join("specs"), &palette).unwrap();
        let replays = root.join("fixtures/replays");
        let state = crate::AppState::with_runs(
            specs,
            palette,
            RunsConfig {
                plant: PlantSource::Scripted {
                    readings: Vec::new(),
                    pace: std::time::Duration::ZERO,
                },
                records: None,
                replays: Some(replays.clone()),
                replay_pace: std::time::Duration::ZERO,
            },
        );
        let events = crate::replay::read(&replays, "home-day108-sequence").unwrap();
        let f = crate::run_facts::facts(&state, "x", &events).await.unwrap();
        assert_eq!(f["this_run"]["electricity_kwh"], 314.3);
        assert_eq!(f["this_run"]["cost_eur"], 72.59);
        assert_eq!(f["peak_price_power"]["kw"], 0.87);
        let vs = &f["vs_baseline"];
        assert_eq!(vs["compared_with"], "replay:home-day108-baseline");
        assert_eq!(vs["peak_price_power_kw_baseline"], 1.17);
        assert_eq!(vs["peak_price_power_change_percent"], -25.7);
        assert_eq!(vs["highest_15_min_draw_change_percent"], 16.4);
        assert_eq!(vs["change_negative_is_less"]["kg_co2e_percent"], -7.7);
        let b = &f["setpoint_behavior"];
        let total = b["preheat_hours"].as_f64().unwrap()
            + b["coast_hours"].as_f64().unwrap()
            + b["normal_hours"].as_f64().unwrap();
        assert_eq!(total, 336.0, "{b}");
        assert!(b["preheat_hours"].as_f64().unwrap() > 0.0, "{b}");
        assert!(b["coast_hours"].as_f64().unwrap() > 0.0, "{b}");
        // Preheating happens when power is cheaper than when coasting.
        assert!(
            b["mean_price_eur_per_kwh_while_preheating"]
                .as_f64()
                .unwrap()
                < b["mean_price_eur_per_kwh_while_coasting"].as_f64().unwrap(),
            "{b}"
        );
        // A baseline gets its own totals and no setpoint behavior.
        let base = crate::replay::read(&replays, "home-day108-baseline").unwrap();
        let fb = crate::run_facts::facts(&state, "y", &base).await.unwrap();
        assert!(fb["setpoint_behavior"].is_null());
        assert!(fb["vs_baseline"].is_null());
    }
}
