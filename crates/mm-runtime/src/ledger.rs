//! Scope 1 and Scope 2 energy, emissions and cost of one run, from BOPTEST's own KPIs and the
//! power readings recorded with every tick. Pure and deterministic: no I/O, no clock, ordered
//! maps only. Shown to people; nothing reads it to decide a control action.
//!
//! Source of every constant: BOPTEST v0.9.0, test case `multizone_office_complex_air`.
//! - The 13 electric and 1 gas signals BOPTEST integrates: `models/kpis.json` (`ElectricPower`,
//!   `GasPower`).
//! - Floor area for the per-m2 KPIs: `models/config.json` (`area`).
//! - Emission factors: `models/Resources/emissions.csv` (constant over the year).
//! - Prices: `models/Resources/prices.csv`, `PriceElectricPowerConstant` and `PriceGasPower`
//!   (the default `Constant` scenario; mm-runtime never selects another).
//! - Integration: `kpis/kpi_calculator.py`, `get_energy` (trapezoid of W over s, times
//!   2.77778e-7 to kWh, divided by the area), `get_cost`, `get_emissions`.
//!
//! Because both factors are constant over the year, the electricity / gas split is exact from
//! two KPI totals: `emis = 0.341 e + 0.18108 g` and `ener = e + g` (per m2). Re-pricing the split
//! must give back BOPTEST's `cost_tot`; [`Ledger::reconciles`] says whether it did. `pele_tot`
//! and `pgas_tot` are peak demands, not energy, and are never added in.

use std::collections::BTreeMap;

use crate::preset::FLOORS;

/// Conditioned floor area BOPTEST divides every energy KPI by (m2).
pub const AREA_M2: f64 = 42_757.0;
/// kgCO2 per kWh of electricity (Scope 2), BOPTEST's average factor for the test case.
pub const ELEC_KG_PER_KWH: f64 = 0.341;
/// kgCO2 per kWh of natural gas burnt on site (Scope 1).
pub const GAS_KG_PER_KWH: f64 = 0.18108;
/// USD per kWh of electricity, `Constant` price scenario.
pub const ELEC_USD_PER_KWH: f64 = 0.094;
/// USD per kWh of gas.
pub const GAS_USD_PER_KWH: f64 = 0.024;
/// BOPTEST's own J to kWh factor (`kpi_calculator.py`), kept so our integral matches its own.
const KWH_PER_J: f64 = 2.77778e-7;
/// Relative tolerance for the re-priced cost against `cost_tot` (the split is exact algebra,
/// so only float rounding is allowed).
const COST_TOLERANCE: f64 = 1e-6;

/// The energy carrier, which fixes the scope: gas is burnt on site (Scope 1), electricity is
/// bought (Scope 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Carrier {
    Electricity,
    Gas,
}

impl Carrier {
    pub fn kg_per_kwh(self) -> f64 {
        match self {
            Carrier::Electricity => ELEC_KG_PER_KWH,
            Carrier::Gas => GAS_KG_PER_KWH,
        }
    }

    pub fn usd_per_kwh(self) -> f64 {
        match self {
            Carrier::Electricity => ELEC_USD_PER_KWH,
            Carrier::Gas => GAS_USD_PER_KWH,
        }
    }

    /// GHG Protocol scope: 1 for on-site combustion, 2 for purchased electricity.
    pub fn scope(self) -> u8 {
        match self {
            Carrier::Electricity => 2,
            Carrier::Gas => 1,
        }
    }
}

/// One end use: the BOPTEST power signals it sums.
#[derive(Debug, Clone, PartialEq)]
pub struct EndUseDef {
    pub id: &'static str,
    pub label: &'static str,
    pub carrier: Carrier,
    pub points: Vec<String>,
}

/// BOPTEST's 14 KPI power signals, grouped into end uses, in display order.
pub fn end_uses() -> Vec<EndUseDef> {
    let per_floor = |p: &str| FLOORS.iter().map(|f| format!("floor{f}_{p}")).collect();
    let one = |p: &str| vec![p.to_owned()];
    let def = |id, label, carrier, points| EndUseDef {
        id,
        label,
        carrier,
        points,
    };
    use Carrier::{Electricity, Gas};
    vec![
        def(
            "chillers",
            "Chillers",
            Electricity,
            one("reaChiWatSys_reaPChi_y"),
        ),
        def(
            "chw_pumps",
            "Chilled-water pumps",
            Electricity,
            one("reaChiWatSys_reaPPum_y"),
        ),
        def(
            "cooling_towers",
            "Cooling towers",
            Electricity,
            one("reaChiWatSys_reaPCooTow_y"),
        ),
        def(
            "supply_fans",
            "Supply fans",
            Electricity,
            per_floor("reaAHU_PFanSup_y"),
        ),
        def(
            "return_fans",
            "Return fans",
            Electricity,
            per_floor("reaAHU_PFanRet_y"),
        ),
        def(
            "freeze_coils",
            "Freeze-protection coils",
            Electricity,
            per_floor("reaAHU_PFreCoi_y"),
        ),
        def(
            "hw_pump",
            "Hot-water pump",
            Electricity,
            one("reaHotWatSys_reaPPum_y"),
        ),
        def("boiler", "Boiler (gas)", Gas, one("reaHotWatSys_reaPBoi_y")),
    ]
}

/// Every KPI power signal, sorted.
pub fn kpi_points() -> Vec<String> {
    let mut all: Vec<String> = end_uses().into_iter().flat_map(|e| e.points).collect();
    all.sort();
    all
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LedgerError {
    #[error("BOPTEST's KPIs are missing {0}")]
    MissingKpi(&'static str),
    #[error("BOPTEST's KPI {0} is not finite")]
    NotFinite(&'static str),
}

/// One row of the ledger: an end use, or what the KPI total has beyond the measured end uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// An [`end_uses`] id, or `unattributed_electricity` / `unattributed_gas`.
    pub id: String,
    pub label: String,
    pub carrier: Carrier,
    pub kwh: f64,
    /// False for the unattributed rows.
    pub measured: bool,
}

impl Line {
    pub fn kg_co2e(&self) -> f64 {
        self.kwh * self.carrier.kg_per_kwh()
    }
    pub fn usd(&self) -> f64 {
        self.kwh * self.carrier.usd_per_kwh()
    }
}

/// A run's energy by carrier (exact, from the KPIs) and, when every signal was recorded, by end
/// use (trapezoid over the recorded ticks).
#[derive(Debug, Clone, PartialEq)]
pub struct Ledger {
    pub electricity_kwh: f64,
    pub gas_kwh: f64,
    /// BOPTEST's `cost_tot` times the area.
    pub boptest_cost_usd: f64,
    /// The split re-priced at the constant prices; equals `boptest_cost_usd` when it reconciles.
    pub repriced_cost_usd: f64,
    /// The split re-prices to BOPTEST's cost and neither carrier is negative.
    pub reconciles: bool,
    /// Every KPI signal was recorded in every tick, so `end_uses` is filled in.
    pub measured: bool,
    /// KPI signals missing from at least one tick (sorted); empty when `measured`.
    pub missing: Vec<String>,
    /// End uses then the two unattributed rows; empty when not `measured`.
    pub end_uses: Vec<Line>,
    /// BOPTEST's thermal discomfort (K h per zone) and CO2 discomfort (ppm h per zone).
    pub tdis_tot: Option<f64>,
    pub idis_tot: Option<f64>,
}

impl Ledger {
    pub fn kwh(&self, carrier: Carrier) -> f64 {
        match carrier {
            Carrier::Electricity => self.electricity_kwh,
            Carrier::Gas => self.gas_kwh,
        }
    }
    pub fn kg_co2e(&self, carrier: Carrier) -> f64 {
        self.kwh(carrier) * carrier.kg_per_kwh()
    }
    pub fn usd(&self, carrier: Carrier) -> f64 {
        self.kwh(carrier) * carrier.usd_per_kwh()
    }
}

fn kpi(kpis: &BTreeMap<String, f64>, name: &'static str) -> Result<f64, LedgerError> {
    let v = *kpis.get(name).ok_or(LedgerError::MissingKpi(name))?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(LedgerError::NotFinite(name))
    }
}

/// Trapezoid of one signal (W) over the samples, in kWh, like BOPTEST's `np.trapezoid`. `None`
/// unless the signal is finite in every sample and there are at least two.
fn integrate(samples: &[(f64, &BTreeMap<String, f64>)], point: &str) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let mut joules = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for (t, readings) in samples {
        let w = *readings.get(point).filter(|v| v.is_finite())?;
        if let Some((t0, w0)) = prev {
            joules += (t - t0) * (w + w0) / 2.0;
        }
        prev = Some((*t, w));
    }
    Some(joules * KWH_PER_J)
}

/// The ledger from BOPTEST's KPI totals (`ener_tot`, `emis_tot`, `cost_tot`, per m2) and the
/// recorded ticks as `(simulation time s, readings)` in time order.
///
/// The ticks hold the readings each step started from, so the last step's interval is not in
/// them: it, and anything BOPTEST's finer integration sees between 120 s samples, lands in the
/// unattributed rows.
pub fn compute(
    kpis: &BTreeMap<String, f64>,
    samples: &[(f64, &BTreeMap<String, f64>)],
) -> Result<Ledger, LedgerError> {
    let ener = kpi(kpis, "ener_tot")?;
    let emis = kpi(kpis, "emis_tot")?;
    let cost = kpi(kpis, "cost_tot")?;
    let elec_m2 = (emis - GAS_KG_PER_KWH * ener) / (ELEC_KG_PER_KWH - GAS_KG_PER_KWH);
    let gas_m2 = ener - elec_m2;
    let electricity_kwh = elec_m2 * AREA_M2;
    let gas_kwh = gas_m2 * AREA_M2;
    let boptest_cost_usd = cost * AREA_M2;
    let repriced_cost_usd = electricity_kwh * ELEC_USD_PER_KWH + gas_kwh * GAS_USD_PER_KWH;
    let scale = boptest_cost_usd.abs().max(1e-9);
    // A carrier may come out a hair below zero from rounding when it is truly zero.
    let slack = 1e-6 * (electricity_kwh.abs() + gas_kwh.abs()).max(1.0);
    let reconciles = (repriced_cost_usd - boptest_cost_usd).abs() <= COST_TOLERANCE * scale
        && electricity_kwh >= -slack
        && gas_kwh >= -slack;

    let mut by_point: BTreeMap<String, f64> = BTreeMap::new();
    let mut missing = Vec::new();
    for p in kpi_points() {
        match integrate(samples, &p) {
            Some(kwh) => {
                by_point.insert(p, kwh);
            }
            None => missing.push(p),
        }
    }
    let measured = missing.is_empty();
    let mut end_uses = Vec::new();
    if measured {
        let mut sums: BTreeMap<Carrier, f64> = BTreeMap::new();
        for def in self::end_uses() {
            let kwh: f64 = def.points.iter().filter_map(|p| by_point.get(p)).sum();
            *sums.entry(def.carrier).or_default() += kwh;
            end_uses.push(Line {
                id: def.id.to_owned(),
                label: def.label.to_owned(),
                carrier: def.carrier,
                kwh,
                measured: true,
            });
        }
        for (carrier, id, label, total) in [
            (
                Carrier::Electricity,
                "unattributed_electricity",
                "Electricity not in the 120 s samples",
                electricity_kwh,
            ),
            (
                Carrier::Gas,
                "unattributed_gas",
                "Gas not in the 120 s samples",
                gas_kwh,
            ),
        ] {
            end_uses.push(Line {
                id: id.to_owned(),
                label: label.to_owned(),
                carrier,
                kwh: total - sums.get(&carrier).copied().unwrap_or_default(),
                measured: false,
            });
        }
    }
    Ok(Ledger {
        electricity_kwh,
        gas_kwh,
        boptest_cost_usd,
        repriced_cost_usd,
        reconciles,
        measured,
        missing,
        end_uses,
        tdis_tot: kpis.get("tdis_tot").copied().filter(|v| v.is_finite()),
        idis_tot: kpis.get("idis_tot").copied().filter(|v| v.is_finite()),
    })
}

/// `b − a`, line by line. End uses only when both ledgers are measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    pub electricity_kwh: f64,
    pub gas_kwh: f64,
    pub end_uses: Vec<Line>,
    pub tdis_tot: Option<f64>,
    pub idis_tot: Option<f64>,
}

impl Delta {
    pub fn kwh(&self, carrier: Carrier) -> f64 {
        match carrier {
            Carrier::Electricity => self.electricity_kwh,
            Carrier::Gas => self.gas_kwh,
        }
    }
}

pub fn delta(a: &Ledger, b: &Ledger) -> Delta {
    let end_uses = if a.measured && b.measured {
        b.end_uses
            .iter()
            .filter_map(|lb| {
                let la = a.end_uses.iter().find(|l| l.id == lb.id)?;
                Some(Line {
                    kwh: lb.kwh - la.kwh,
                    ..lb.clone()
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let diff = |x: Option<f64>, y: Option<f64>| Some(y? - x?);
    Delta {
        electricity_kwh: b.electricity_kwh - a.electricity_kwh,
        gas_kwh: b.gas_kwh - a.gas_kwh,
        end_uses,
        tdis_tot: diff(a.tdis_tot, b.tdis_tot),
        idis_tot: diff(a.idis_tot, b.idis_tot),
    }
}
