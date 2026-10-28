//! A run's Scope 1 and Scope 2 ledger as the web app and the MCP tools see it, built from the
//! run's events (a live run's journal or a recorded run's file) by [`mm_runtime::ledger`].
//! Simulation results under BOPTEST's own factors and prices; never used for control.

use std::path::Path;

use mm_runtime::ledger::{self, Basis, Carrier, Ledger, Line, Prices};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::run_dto::{Envelope, Fault, RunConfig, RunEvent};

/// The factors and prices every number here uses, and where they come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LedgerFactors {
    pub electricity_kg_per_kwh: f64,
    /// Absent when the building burns no fuel on site.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gas_kg_per_kwh: Option<f64>,
    /// Constant prices per kWh; absent when the price varies (the home's spot prices).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub electricity_price_per_kwh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gas_price_per_kwh: Option<f64>,
    /// ISO code every cost here is in (BOPTEST's own for the test case).
    pub currency: String,
    /// Floor area BOPTEST divides its KPIs by (m2).
    pub area_m2: f64,
    pub source: String,
}

impl LedgerFactors {
    pub fn of(basis: &Basis) -> Self {
        let (e, g) = match basis.prices {
            Prices::Constant { electricity, gas } => (Some(electricity), Some(gas)),
            Prices::Varying => (None, None),
        };
        Self {
            electricity_kg_per_kwh: basis.electricity_kg_per_kwh,
            gas_kg_per_kwh: basis.gas_kg_per_kwh,
            electricity_price_per_kwh: e,
            gas_price_per_kwh: g.filter(|_| basis.gas_kg_per_kwh.is_some()),
            currency: basis.currency.to_owned(),
            area_m2: basis.area_m2,
            source: basis.source.to_owned(),
        }
    }

    /// The office's factors.
    pub fn boptest() -> Self {
        Self::of(&Basis::office())
    }
}

/// Energy, emissions and cost of one carrier, or their sum, or a change in them. Cost is in the
/// ledger's currency.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Amounts {
    pub kwh: f64,
    pub kg_co2e: f64,
    pub cost: f64,
}

impl Amounts {
    fn plus(self, o: Self) -> Self {
        Self {
            kwh: self.kwh + o.kwh,
            kg_co2e: self.kg_co2e + o.kg_co2e,
            cost: self.cost + o.cost,
        }
    }

    fn minus(self, o: Self) -> Self {
        Self {
            kwh: self.kwh - o.kwh,
            kg_co2e: self.kg_co2e - o.kg_co2e,
            cost: self.cost - o.cost,
        }
    }

    fn of_ledger(l: &Ledger, carrier: Carrier) -> Self {
        Self {
            kwh: l.kwh(carrier),
            kg_co2e: l.kg_co2e(carrier),
            cost: l.cost(carrier),
        }
    }
}

/// One end use, or what the KPI total holds beyond the measured ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EndUseView {
    pub id: String,
    pub label: String,
    /// 1 (gas burnt on site) or 2 (electricity bought).
    pub scope: u8,
    pub amounts: Amounts,
    /// False for the unattributed rows.
    pub measured: bool,
}

impl From<&Line> for EndUseView {
    fn from(l: &Line) -> Self {
        Self {
            id: l.id.clone(),
            label: l.label.clone(),
            scope: l.carrier.scope(),
            amounts: Amounts {
                kwh: l.kwh,
                kg_co2e: l.kg_co2e,
                cost: l.cost,
            },
            measured: l.measured,
        }
    }
}

/// Which run a ledger is of, and its window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LedgerSubject {
    /// `run:<id>` or `replay:<record name>`.
    pub reference: String,
    pub case: String,
    pub start_s: f64,
    pub steps: u32,
    pub step_s: f64,
    /// Ticks the run actually recorded (fewer than `steps` when it stopped early).
    pub ticks: u32,
    pub baseline: bool,
    pub requests: bool,
    pub pressure: bool,
    /// Spec name to the content id of the drafted graph it ran from.
    pub drafts: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fault: Option<Fault>,
    /// The record a replayed run played back.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replay_of: Option<String>,
}

/// A run's energy, emissions and cost, by scope and (when recorded) by end use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunLedger {
    pub subject: LedgerSubject,
    pub factors: LedgerFactors,
    /// Fuel burnt on site (the office's boiler); zero for an all-electric building.
    pub scope1: Amounts,
    /// Electricity bought.
    pub scope2: Amounts,
    pub total: Amounts,
    /// BOPTEST's own operating cost KPI for the run, in the factors' currency.
    pub boptest_cost: f64,
    /// BOPTEST's cost over the run's energy: what each kWh cost on average.
    pub average_price_per_kwh: f64,
    /// The split checks against BOPTEST (constant prices: re-prices to its cost; varying
    /// prices: its emissions equal the energy times the factor) and neither scope is negative.
    pub reconciles: bool,
    /// True when every KPI power signal was recorded in every tick, so `end_uses` is filled.
    pub measured: bool,
    /// KPI signals missing from the record (older records have none of the fan or boiler ones).
    pub missing: Vec<String>,
    pub end_uses: Vec<EndUseView>,
    /// BOPTEST's thermal discomfort, K h per zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tdis_tot: Option<f64>,
    /// BOPTEST's CO2 discomfort, ppm h per zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub idis_tot: Option<f64>,
}

/// `b − a`: positive numbers mean `b` used, emitted or cost more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LedgerDelta {
    pub scope1: Amounts,
    pub scope2: Amounts,
    pub total: Amounts,
    /// Per end use, only when both runs recorded every signal.
    pub end_uses: Vec<EndUseView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tdis_tot: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub idis_tot: Option<f64>,
}

/// Two ledgers of the same window and how `b` differs from `a`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LedgerComparison {
    pub a: RunLedger,
    pub b: RunLedger,
    pub delta: LedgerDelta,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LedgerFailure {
    #[error("the run has not started yet")]
    NotStarted,
    #[error("the run has no BOPTEST KPIs yet; they arrive when it ends")]
    NoKpis,
    #[error("{0}")]
    Kpis(#[from] ledger::LedgerError),
    #[error("{0}")]
    WindowMismatch(String),
    #[error("no ledger basis for test case {0}")]
    OtherBuilding(String),
}

impl LedgerFailure {
    /// The stable code the HTTP API and MCP tools report.
    pub fn code(&self) -> &'static str {
        match self {
            LedgerFailure::NotStarted => "not-started",
            LedgerFailure::NoKpis => "no-kpis",
            LedgerFailure::Kpis(_) => "bad-kpis",
            LedgerFailure::WindowMismatch(_) => "window-mismatch",
            LedgerFailure::OtherBuilding(_) => "other-building",
        }
    }
}

fn subject(reference: &str, config: &RunConfig, ticks: u32) -> LedgerSubject {
    LedgerSubject {
        reference: reference.to_owned(),
        case: config.case.clone(),
        start_s: config.start_s,
        steps: config.steps,
        step_s: config.step_s,
        ticks,
        baseline: config.baseline,
        requests: config.requests,
        pressure: config.pressure,
        drafts: config.drafts.clone(),
        fault: config.fault.clone(),
        replay_of: config.replay.as_ref().map(|r| r.record.clone()),
    }
}

/// The ledger of a run from its events: its config, the last KPI report, every tick.
pub fn from_events(reference: &str, events: &[Envelope]) -> Result<RunLedger, LedgerFailure> {
    let config = events
        .iter()
        .find_map(|e| match &e.event {
            RunEvent::Started { config, .. } => Some(config),
            _ => None,
        })
        .ok_or(LedgerFailure::NotStarted)?;
    let basis = Basis::for_case(&config.case)
        .ok_or_else(|| LedgerFailure::OtherBuilding(config.case.clone()))?;
    let kpis = events
        .iter()
        .rev()
        .find_map(|e| match &e.event {
            RunEvent::Kpi { values } => Some(values),
            _ => None,
        })
        .ok_or(LedgerFailure::NoKpis)?;
    let samples: Vec<(f64, &std::collections::BTreeMap<String, f64>)> = events
        .iter()
        .filter_map(|e| match (&e.event, e.sim_time_s) {
            (RunEvent::Tick { readings, .. }, Some(t)) => Some((t, readings)),
            _ => None,
        })
        .collect();
    let ticks = events
        .iter()
        .filter(|e| matches!(e.event, RunEvent::Tick { .. }))
        .count();
    let l = ledger::compute_for(&basis, kpis, &samples)?;
    Ok(view(
        &basis,
        subject(reference, config, u32::try_from(ticks).unwrap_or(u32::MAX)),
        &l,
    ))
}

fn view(basis: &Basis, subject: LedgerSubject, l: &Ledger) -> RunLedger {
    let scope1 = Amounts::of_ledger(l, Carrier::Gas);
    let scope2 = Amounts::of_ledger(l, Carrier::Electricity);
    let total = scope1.plus(scope2);
    RunLedger {
        subject,
        factors: LedgerFactors::of(basis),
        scope1,
        scope2,
        total,
        boptest_cost: l.boptest_cost,
        average_price_per_kwh: if total.kwh > 0.0 {
            l.boptest_cost / total.kwh
        } else {
            0.0
        },
        reconciles: l.reconciles,
        measured: l.measured,
        missing: l.missing.clone(),
        end_uses: l.end_uses.iter().map(EndUseView::from).collect(),
        tdis_tot: l.tdis_tot,
        idis_tot: l.idis_tot,
    }
}

/// Why two runs cannot be compared, if they cannot: same test case, start, step length, steps
/// asked for and ticks recorded.
pub fn window_mismatch(a: &LedgerSubject, b: &LedgerSubject) -> Option<String> {
    let mut why = Vec::new();
    if a.case != b.case {
        why.push(format!("test case {} vs {}", a.case, b.case));
    }
    if a.start_s.to_bits() != b.start_s.to_bits() {
        why.push(format!("start {} s vs {} s", a.start_s, b.start_s));
    }
    if a.step_s.to_bits() != b.step_s.to_bits() {
        why.push(format!("step {} s vs {} s", a.step_s, b.step_s));
    }
    if a.steps != b.steps {
        why.push(format!("{} steps vs {}", a.steps, b.steps));
    }
    if a.ticks != b.ticks {
        why.push(format!("{} ticks recorded vs {}", a.ticks, b.ticks));
    }
    (!why.is_empty()).then(|| {
        format!(
            "{} and {} are not the same window: {}",
            a.reference,
            b.reference,
            why.join(", ")
        )
    })
}

/// `b − a`, refused unless both cover the same window.
pub fn compare(a: RunLedger, b: RunLedger) -> Result<LedgerComparison, LedgerFailure> {
    if let Some(why) = window_mismatch(&a.subject, &b.subject) {
        return Err(LedgerFailure::WindowMismatch(why));
    }
    // End uses only when both runs recorded every signal.
    let end_uses = if a.measured && b.measured {
        b.end_uses
            .iter()
            .filter_map(|eb| {
                let ea = a.end_uses.iter().find(|e| e.id == eb.id)?;
                Some(EndUseView {
                    amounts: eb.amounts.minus(ea.amounts),
                    ..eb.clone()
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let diff = |x: Option<f64>, y: Option<f64>| Some(y? - x?);
    let delta = LedgerDelta {
        scope1: b.scope1.minus(a.scope1),
        scope2: b.scope2.minus(a.scope2),
        total: b.total.minus(a.total),
        end_uses,
        tdis_tot: diff(a.tdis_tot, b.tdis_tot),
        idis_tot: diff(a.idis_tot, b.idis_tot),
    };
    Ok(LedgerComparison { a, b, delta })
}

/// The ledger of `<dir>/<name>.jsonl`, a recorded run.
pub fn of_record(dir: &Path, name: &str) -> Result<RunLedger, RecordLedgerError> {
    let events = crate::replay::read(dir, name).map_err(RecordLedgerError::Record)?;
    from_events(&format!("replay:{name}"), &events).map_err(RecordLedgerError::Ledger)
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RecordLedgerError {
    /// The record is missing or unreadable.
    #[error("{0}")]
    Record(String),
    #[error(transparent)]
    Ledger(LedgerFailure),
}
