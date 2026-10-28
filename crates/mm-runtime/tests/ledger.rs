//! The emissions ledger: exact carrier split from BOPTEST's KPIs, end uses from recorded ticks.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use mm_runtime::ledger::{self, Carrier};

/// BOPTEST's KPIs of the recorded day-143 baseline (`fixtures/replays`).
fn day143_baseline() -> BTreeMap<String, f64> {
    [
        ("cost_tot", 0.022534986481290514),
        ("emis_tot", 0.10171230167699279),
        ("ener_tot", 0.39785664748724936),
        ("tdis_tot", 0.11920257618624386),
        ("idis_tot", 899.3128443343401),
        ("pele_tot", 0.031702624994601476),
        ("pgas_tot", 0.09175732592703592),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

#[test]
fn the_kpi_signals_are_boptests_fourteen() {
    // models/kpis.json of multizone_office_complex_air: 13 ElectricPower, 1 GasPower.
    let mut expected: Vec<String> = [
        "reaChiWatSys_reaPPum_y",
        "reaChiWatSys_reaPChi_y",
        "reaChiWatSys_reaPCooTow_y",
        "reaHotWatSys_reaPPum_y",
        "reaHotWatSys_reaPBoi_y",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for f in 1..=3 {
        for p in ["PFanRet", "PFanSup", "PFreCoi"] {
            expected.push(format!("floor{f}_reaAHU_{p}_y"));
        }
    }
    expected.sort();
    assert_eq!(ledger::kpi_points(), expected);
    let gas: Vec<_> = ledger::end_uses()
        .into_iter()
        .filter(|e| e.carrier == Carrier::Gas)
        .flat_map(|e| e.points)
        .collect();
    assert_eq!(gas, ["reaHotWatSys_reaPBoi_y"]);
}

#[test]
fn the_split_is_exact_and_reprices_to_boptests_cost() {
    let l = ledger::compute(&day143_baseline(), &[]).unwrap();
    assert_eq!(l.electricity_kwh.round(), 7932.0);
    assert_eq!(l.gas_kwh.round(), 9079.0);
    assert_eq!(l.kg_co2e(Carrier::Electricity).round(), 2705.0);
    assert_eq!(l.kg_co2e(Carrier::Gas).round(), 1644.0);
    assert!(l.reconciles);
    assert!((l.repriced_cost_usd - 963.53).abs() < 0.01, "{l:?}");
    assert!((l.boptest_cost_usd - l.repriced_cost_usd).abs() < 1e-9);
    assert!(!l.measured, "no ticks, no end uses");
    assert!(l.end_uses.is_empty());
    assert_eq!(l.missing.len(), 14);
    // Peak demand (pele_tot, pgas_tot) is not energy: doubling it changes nothing.
    let mut k = day143_baseline();
    k.insert("pele_tot".into(), 1.0);
    assert_eq!(ledger::compute(&k, &[]).unwrap(), l);
}

#[test]
fn a_cost_that_does_not_reprice_is_flagged() {
    let mut k = day143_baseline();
    k.insert("cost_tot".into(), 0.03);
    assert!(!ledger::compute(&k, &[]).unwrap().reconciles);
}

#[test]
fn missing_kpis_are_typed_errors() {
    let mut k = day143_baseline();
    k.remove("emis_tot");
    assert_eq!(
        ledger::compute(&k, &[]),
        Err(ledger::LedgerError::MissingKpi("emis_tot"))
    );
    k.insert("emis_tot".into(), f64::NAN);
    assert_eq!(
        ledger::compute(&k, &[]),
        Err(ledger::LedgerError::NotFinite("emis_tot"))
    );
}

/// Constant power on every signal for three samples 120 s apart.
fn ticks(watts: f64) -> Vec<BTreeMap<String, f64>> {
    let r: BTreeMap<String, f64> = ledger::kpi_points()
        .into_iter()
        .map(|p| (p, watts))
        .collect();
    vec![r.clone(), r.clone(), r]
}

#[test]
fn end_uses_integrate_like_boptest_and_the_rest_is_unattributed() {
    let t = ticks(1000.0);
    let samples: Vec<_> = t
        .iter()
        .enumerate()
        .map(|(i, r)| (i as f64 * 120.0, r))
        .collect();
    let l = ledger::compute(&day143_baseline(), &samples).unwrap();
    assert!(l.measured);
    let ids: Vec<_> = l.end_uses.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "chillers",
            "chw_pumps",
            "cooling_towers",
            "supply_fans",
            "return_fans",
            "freeze_coils",
            "hw_pump",
            "boiler",
            "unattributed_electricity",
            "unattributed_gas"
        ]
    );
    // 1 kW for 240 s = 1/15 kWh per signal (with BOPTEST's own J to kWh factor).
    let one = 240_000.0 * 2.77778e-7;
    assert!((l.end_uses[0].kwh - one).abs() < 1e-12);
    assert!(
        (l.end_uses[3].kwh - 3.0 * one).abs() < 1e-12,
        "three floors"
    );
    let boiler = &l.end_uses[7];
    assert_eq!(boiler.carrier, Carrier::Gas);
    assert!((boiler.kg_co2e() - one * 0.18108).abs() < 1e-12);
    let unattributed_gas = &l.end_uses[9];
    assert!(!unattributed_gas.measured);
    assert!((unattributed_gas.kwh - (l.gas_kwh - one)).abs() < 1e-9);
    // Every row of a carrier adds up to the exact KPI total.
    let elec: f64 = l
        .end_uses
        .iter()
        .filter(|e| e.carrier == Carrier::Electricity)
        .map(|e| e.kwh)
        .sum();
    assert!((elec - l.electricity_kwh).abs() < 1e-9);
}

#[test]
fn a_signal_missing_from_one_tick_means_no_end_uses() {
    let mut t = ticks(1000.0);
    t[1].remove("floor2_reaAHU_PFreCoi_y");
    let samples: Vec<_> = t
        .iter()
        .enumerate()
        .map(|(i, r)| (i as f64 * 120.0, r))
        .collect();
    let l = ledger::compute(&day143_baseline(), &samples).unwrap();
    assert!(!l.measured);
    assert!(l.end_uses.is_empty());
    assert_eq!(l.missing, ["floor2_reaAHU_PFreCoi_y"]);
}

#[test]
fn a_delta_is_b_minus_a_and_deterministic() {
    let t = ticks(1000.0);
    let s: Vec<_> = t
        .iter()
        .enumerate()
        .map(|(i, r)| (i as f64 * 120.0, r))
        .collect();
    let a = ledger::compute(&day143_baseline(), &s).unwrap();
    let mut kb = day143_baseline();
    kb.insert("ener_tot".into(), 0.24945800914857538);
    kb.insert("emis_tot".into(), 0.06862729816291906);
    kb.insert("cost_tot".into(), 0.01625388148070045);
    kb.insert("tdis_tot".into(), 0.1064718890506822);
    let b = ledger::compute(&kb, &s).unwrap();
    let d = ledger::delta(&a, &b);
    assert_eq!(d.kwh(Carrier::Gas).round(), (4395.0 - 9079.0_f64).round());
    assert!((d.tdis_tot.unwrap() - (0.1064718890506822 - 0.11920257618624386)).abs() < 1e-15);
    assert_eq!(d.end_uses.len(), 10);
    assert_eq!(d.end_uses[0].kwh, 0.0, "same readings, no change");
    assert_eq!(ledger::delta(&a, &b), d);
    // An unmeasured side gives scope deltas only.
    let bare = ledger::compute(&kb, &[]).unwrap();
    assert!(ledger::delta(&a, &bare).end_uses.is_empty());
}

/// Samples `(t, readings)` with every KPI signal at `watts(point, k)` at t = 120 k.
fn samples_by(n: usize, watts: impl Fn(&str, usize) -> f64) -> Vec<BTreeMap<String, f64>> {
    (0..n)
        .map(|k| {
            ledger::kpi_points()
                .into_iter()
                .map(|p| {
                    let w = watts(&p, k);
                    (p, w)
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_ramp_integrates_by_the_trapezoid_rule() {
    // Chiller power 0, 1000, 3000 W at 0, 120, 240 s; everything else off. Trapezoid:
    // 120 * (0 + 1000) / 2 + 120 * (1000 + 3000) / 2 = 300,000 J (a left Riemann sum gives
    // 120,000 J, a right one 480,000 J).
    let chiller = [0.0, 1000.0, 3000.0];
    let t = samples_by(3, |p, k| {
        if p == "reaChiWatSys_reaPChi_y" {
            chiller[k]
        } else {
            0.0
        }
    });
    let s: Vec<_> = t
        .iter()
        .enumerate()
        .map(|(i, r)| (i as f64 * 120.0, r))
        .collect();
    let l = ledger::compute(&day143_baseline(), &s).unwrap();
    let chillers = l.end_uses.iter().find(|e| e.id == "chillers").unwrap();
    assert!(
        (chillers.kwh - 300_000.0 * 2.77778e-7).abs() < 1e-12,
        "{}",
        chillers.kwh
    );
    let fans = l.end_uses.iter().find(|e| e.id == "supply_fans").unwrap();
    assert_eq!(fans.kwh, 0.0);
}

#[test]
fn an_end_use_delta_is_b_minus_a_with_its_sign() {
    let at = |boiler: f64, chiller: f64| {
        samples_by(3, move |p, _| match p {
            "reaHotWatSys_reaPBoi_y" => boiler,
            "reaChiWatSys_reaPChi_y" => chiller,
            _ => 100.0,
        })
    };
    let (ta, tb) = (at(50_000.0, 10_000.0), at(20_000.0, 12_000.0));
    let timed = |t: &[BTreeMap<String, f64>]| -> ledger::Ledger {
        let s: Vec<_> = t
            .iter()
            .enumerate()
            .map(|(i, r)| (i as f64 * 120.0, r))
            .collect();
        ledger::compute(&day143_baseline(), &s).unwrap()
    };
    let (a, b) = (timed(&ta), timed(&tb));
    let d = ledger::delta(&a, &b);
    let by = |id: &str| d.end_uses.iter().find(|e| e.id == id).unwrap().kwh;
    let kwh = |w: f64| w * 240.0 * 2.77778e-7;
    assert!(
        (by("boiler") - kwh(20_000.0 - 50_000.0)).abs() < 1e-9,
        "less gas is negative"
    );
    assert!(
        (by("chillers") - kwh(2_000.0)).abs() < 1e-9,
        "more chiller power is positive"
    );
    assert_eq!(by("supply_fans"), 0.0);
    // Same KPI totals, so what the samples gained the unattributed row lost.
    assert!((by("unattributed_gas") + by("boiler")).abs() < 1e-9);
}

/// Every committed recorded run's KPIs split into scopes that re-price exactly to BOPTEST's
/// own cost (docs/emissions.md says so).
#[test]
fn every_committed_record_reconciles() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/replays");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 11);
    for path in names {
        let text = std::fs::read_to_string(&path).unwrap();
        let kpis: BTreeMap<String, f64> = text
            .lines()
            .rev()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v["kind"] == "kpi")
            .map(|v| serde_json::from_value(v["values"].clone()).unwrap())
            .unwrap_or_else(|| panic!("{} has no KPIs", path.display()));
        let l = ledger::compute(&kpis, &[]).unwrap();
        assert!(l.reconciles, "{}: {l:?}", path.display());
        assert!(
            l.electricity_kwh > 0.0 && l.gas_kwh > 0.0,
            "{}",
            path.display()
        );
    }
}
