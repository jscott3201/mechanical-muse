//! Scope 1 and Scope 2 energy, emissions and cost of one run, from BOPTEST's own KPIs and the
//! power readings recorded with every tick. Pure and deterministic: no I/O, no clock, ordered
//! maps only. Shown to people; nothing reads it to decide a control action.
//!
//! Each test case has its own [`Basis`]: the constants below are the office's; the home's (all
//! electric, spot prices) are in `ledger/basis.rs`.
//!
//! Source of every office constant: BOPTEST v0.9.0, test case `multizone_office_complex_air`.
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

mod basis;
pub use basis::{Basis, Prices};

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

/// Every KPI power signal of the office, sorted.
pub fn kpi_points() -> Vec<String> {
    Basis::office().kpi_points()
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
    /// An end use id, or `unattributed_electricity` / `unattributed_gas`.
    pub id: String,
    pub label: String,
    pub carrier: Carrier,
    pub kwh: f64,
    pub kg_co2e: f64,
    /// In the basis's currency: at the constant price, or at the run's average price paid.
    pub cost: f64,
    /// False for the unattributed rows.
    pub measured: bool,
}

/// A run's energy by carrier (exact, from the KPIs) and, when every signal was recorded, by end
/// use (trapezoid over the recorded ticks).
#[derive(Debug, Clone, PartialEq)]
pub struct Ledger {
    pub electricity_kwh: f64,
    pub gas_kwh: f64,
    pub electricity_kg_co2e: f64,
    pub gas_kg_co2e: f64,
    /// Cost per carrier in the basis's currency (all of BOPTEST's cost is electricity when the
    /// building burns nothing).
    pub electricity_cost: f64,
    pub gas_cost: f64,
    /// BOPTEST's `cost_tot` times the area.
    pub boptest_cost: f64,
    /// Constant prices: the split re-prices to BOPTEST's cost. Varying prices: BOPTEST's
    /// emissions equal the energy times the constant factor. Neither carrier is negative.
    pub reconciles: bool,
    /// Every KPI signal was recorded in every tick, so `end_uses` is filled in.
    pub measured: bool,
    /// KPI signals missing from at least one tick (sorted); empty when `measured`.
    pub missing: Vec<String>,
    /// End uses then the unattributed rows; empty when not `measured`.
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
        match carrier {
            Carrier::Electricity => self.electricity_kg_co2e,
            Carrier::Gas => self.gas_kg_co2e,
        }
    }
    pub fn cost(&self, carrier: Carrier) -> f64 {
        match carrier {
            Carrier::Electricity => self.electricity_cost,
            Carrier::Gas => self.gas_cost,
        }
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

/// The office's ledger ([`Basis::office`]); see [`compute_for`].
pub fn compute(
    kpis: &BTreeMap<String, f64>,
    samples: &[(f64, &BTreeMap<String, f64>)],
) -> Result<Ledger, LedgerError> {
    compute_for(&Basis::office(), kpis, samples)
}

/// The ledger from BOPTEST's KPI totals (`ener_tot`, `emis_tot`, `cost_tot`, per m2) and the
/// recorded ticks as `(simulation time s, readings)` in time order.
///
/// The ticks hold the readings each step started from, so the last step's interval is not in
/// them: it, and anything BOPTEST's finer integration sees between samples, lands in the
/// unattributed rows.
pub fn compute_for(
    basis: &Basis,
    kpis: &BTreeMap<String, f64>,
    samples: &[(f64, &BTreeMap<String, f64>)],
) -> Result<Ledger, LedgerError> {
    let ener = kpi(kpis, "ener_tot")?;
    let emis = kpi(kpis, "emis_tot")?;
    let cost = kpi(kpis, "cost_tot")?;
    let area = basis.area_m2;
    let k_elec = basis.electricity_kg_per_kwh;
    // With gas the split solves two equations; a building that burns nothing is all electric.
    let elec_m2 = match basis.gas_kg_per_kwh {
        Some(k_gas) => (emis - k_gas * ener) / (k_elec - k_gas),
        None => ener,
    };
    let electricity_kwh = elec_m2 * area;
    let gas_kwh = (ener - elec_m2) * area;
    let boptest_cost = cost * area;
    let (electricity_cost, gas_cost, priced_ok) = match basis.prices {
        Prices::Constant { electricity, gas } => {
            let (e, g) = (electricity_kwh * electricity, gas_kwh * gas);
            let ok = (e + g - boptest_cost).abs() <= COST_TOLERANCE * boptest_cost.abs().max(1e-9);
            (e, g, ok)
        }
        // Spot prices: BOPTEST's cost is the cost; check the emissions instead.
        Prices::Varying => {
            let ok = (emis * area - k_elec * electricity_kwh).abs()
                <= COST_TOLERANCE * (emis * area).abs().max(1e-9);
            (boptest_cost, 0.0, ok)
        }
    };
    // A carrier may come out a hair below zero from rounding when it is truly zero.
    let slack = 1e-6 * (electricity_kwh.abs() + gas_kwh.abs()).max(1.0);
    let reconciles = priced_ok && electricity_kwh >= -slack && gas_kwh >= -slack;
    let price_of = |carrier: Carrier| match (basis.prices, carrier) {
        (Prices::Constant { electricity, .. }, Carrier::Electricity) => electricity,
        (Prices::Constant { gas, .. }, Carrier::Gas) => gas,
        (Prices::Varying, Carrier::Electricity) if electricity_kwh > 0.0 => {
            electricity_cost / electricity_kwh
        }
        (Prices::Varying, _) => 0.0,
    };
    let line = |id: &str, label: &str, carrier: Carrier, kwh: f64, measured: bool| Line {
        id: id.to_owned(),
        label: label.to_owned(),
        carrier,
        kwh,
        kg_co2e: kwh * basis.kg_per_kwh(carrier),
        cost: kwh * price_of(carrier),
        measured,
    };

    let mut by_point: BTreeMap<String, f64> = BTreeMap::new();
    let mut missing = Vec::new();
    for p in basis.kpi_points() {
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
        for def in basis.end_uses() {
            let kwh: f64 = def.points.iter().filter_map(|p| by_point.get(p)).sum();
            *sums.entry(def.carrier).or_default() += kwh;
            end_uses.push(line(def.id, def.label, def.carrier, kwh, true));
        }
        let mut rest = vec![(
            Carrier::Electricity,
            "unattributed_electricity",
            format!("Electricity not in the {} samples", basis.sample_step),
            electricity_kwh,
        )];
        if basis.gas_kg_per_kwh.is_some() {
            rest.push((
                Carrier::Gas,
                "unattributed_gas",
                format!("Gas not in the {} samples", basis.sample_step),
                gas_kwh,
            ));
        }
        for (carrier, id, label, total) in rest {
            let kwh = total - sums.get(&carrier).copied().unwrap_or_default();
            end_uses.push(line(id, &label, carrier, kwh, false));
        }
    }
    Ok(Ledger {
        electricity_kwh,
        gas_kwh,
        electricity_kg_co2e: electricity_kwh * k_elec,
        gas_kg_co2e: gas_kwh * basis.kg_per_kwh(Carrier::Gas),
        electricity_cost,
        gas_cost,
        boptest_cost,
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
                    kg_co2e: lb.kg_co2e - la.kg_co2e,
                    cost: lb.cost - la.cost,
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
