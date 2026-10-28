//! What a ledger is computed on, per BOPTEST test case: floor area, emission factors, prices and
//! the power signals BOPTEST integrates. Every constant is BOPTEST's own for that case.

use super::{Carrier, EndUseDef};

/// How the run's energy is priced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Prices {
    /// One price per carrier for the whole year (per kWh): re-pricing the split must give back
    /// BOPTEST's cost KPI, which checks the split.
    Constant { electricity: f64, gas: f64 },
    /// A price that changes over time (the home's spot prices): BOPTEST's cost KPI is taken as
    /// is, and end uses are priced at the run's average price paid.
    Varying,
}

/// The constants of one test case.
#[derive(Debug, Clone)]
pub struct Basis {
    pub case: &'static str,
    /// Floor area BOPTEST divides its energy, emissions and cost KPIs by (m2).
    pub area_m2: f64,
    /// kgCO2 per kWh of electricity (Scope 2).
    pub electricity_kg_per_kwh: f64,
    /// kgCO2 per kWh of gas burnt on site (Scope 1); `None` when the building burns nothing.
    pub gas_kg_per_kwh: Option<f64>,
    pub prices: Prices,
    /// ISO code of BOPTEST's cost KPI for the case.
    pub currency: &'static str,
    /// Where the constants come from.
    pub source: &'static str,
    /// How often the runtime samples power, for the unattributed rows' labels.
    pub sample_step: &'static str,
    end_uses: fn() -> Vec<EndUseDef>,
}

impl Basis {
    /// BOPTEST `multizone_office_complex_air`: see the constants in [`super`].
    pub fn office() -> Self {
        Self {
            case: crate::preset::CASE,
            area_m2: super::AREA_M2,
            electricity_kg_per_kwh: super::ELEC_KG_PER_KWH,
            gas_kg_per_kwh: Some(super::GAS_KG_PER_KWH),
            prices: Prices::Constant {
                electricity: super::ELEC_USD_PER_KWH,
                gas: super::GAS_USD_PER_KWH,
            },
            currency: "USD",
            source: "BOPTEST v0.9.0 multizone_office_complex_air: emissions.csv, prices.csv \
                (Constant), config.json, kpis.json",
            sample_step: "120 s",
            end_uses: super::end_uses,
        }
    }

    /// BOPTEST `bestest_hydronic_heat_pump`, the home: all electric (heat pump, its fan, the
    /// floor heating pump; `models/kpis.json`), 192 m2 (`models/config.json`), a constant
    /// 0.167 kgCO2/kWh (`models/Resources/emissions.csv`), and the spot price scenario the
    /// runtime selects (`PriceElectricPowerHighlyDynamic` in `prices.csv`, in euros).
    pub fn home() -> Self {
        Self {
            case: crate::home::CASE,
            area_m2: 192.0,
            electricity_kg_per_kwh: 0.167,
            gas_kg_per_kwh: None,
            prices: Prices::Varying,
            currency: "EUR",
            source: "BOPTEST v0.9.0 bestest_hydronic_heat_pump: emissions.csv, prices.csv \
                (HighlyDynamic), config.json, kpis.json",
            sample_step: "15-min",
            end_uses: home_end_uses,
        }
    }

    /// The basis of a test case, if we have one.
    pub fn for_case(case: &str) -> Option<Self> {
        [Self::office(), Self::home()]
            .into_iter()
            .find(|b| b.case == case)
    }

    pub fn end_uses(&self) -> Vec<EndUseDef> {
        (self.end_uses)()
    }

    /// Every KPI power signal of the case, sorted.
    pub fn kpi_points(&self) -> Vec<String> {
        let mut all: Vec<String> = self.end_uses().into_iter().flat_map(|e| e.points).collect();
        all.sort();
        all
    }

    pub fn kg_per_kwh(&self, carrier: Carrier) -> f64 {
        match carrier {
            Carrier::Electricity => self.electricity_kg_per_kwh,
            Carrier::Gas => self.gas_kg_per_kwh.unwrap_or(0.0),
        }
    }
}

/// The home's three KPI power signals.
fn home_end_uses() -> Vec<EndUseDef> {
    let def = |id, label, point: &str| EndUseDef {
        id,
        label,
        carrier: Carrier::Electricity,
        points: vec![point.to_owned()],
    };
    vec![
        def("heat_pump", "Heat pump compressor", "reaPHeaPum_y"),
        def("hp_fan", "Heat pump fan", "reaPFan_y"),
        def("floor_pump", "Floor heating pump", "reaPPumEmi_y"),
    ]
}
