//! The ledger facts a run explanation may quote: the run's own Scope 1 and 2 totals and, when
//! a run of the same window exists here or in the recorded runs, its change against BOPTEST's
//! baseline and (for a run with a test fault) against the same run without the fault. Numbers
//! are rounded here, once, so the model has nothing to compute (`mm_agent::unsupported_numbers`
//! refuses a reply that states any other number).

use serde_json::{Value, json};

use crate::ledger::{self, Amounts, LedgerComparison, RunLedger};
use crate::routes::AppState;
use crate::run_dto::{Envelope, FaultMode};

fn r0(x: f64) -> f64 {
    x.round()
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn r3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn amounts(a: &Amounts) -> Value {
    json!({"kwh": r0(a.kwh), "kg_co2e": r0(a.kg_co2e), "usd": r0(a.usd)})
}

fn pct(change: f64, base: f64) -> Option<f64> {
    (base.abs() > f64::EPSILON).then(|| r1(change / base.abs() * 100.0))
}

fn totals(l: &RunLedger) -> Value {
    json!({
        "scope1_gas": amounts(&l.scope1),
        "scope2_electricity": amounts(&l.scope2),
        "total": amounts(&l.total),
        "thermal_discomfort_kh_per_zone": l.tdis_tot.map(r3),
    })
}

fn change(c: &LedgerComparison) -> Value {
    let d = &c.delta;
    json!({
        "compared_with": c.a.subject.reference,
        "change_negative_is_less": {
            "scope1_gas": amounts(&d.scope1),
            "scope2_electricity": amounts(&d.scope2),
            "total": amounts(&d.total),
            "total_kg_co2e_percent": pct(d.total.kg_co2e, c.a.total.kg_co2e),
            "total_usd_percent": pct(d.total.usd, c.a.total.usd),
            "thermal_discomfort_kh_per_zone": d.tdis_tot.map(r3),
        },
    })
}

/// Every other run of the same window with KPIs: runs on this server (newest first), then
/// recorded runs (by name), skipping the record this run replays.
async fn same_window(state: &AppState, own: &RunLedger) -> Vec<RunLedger> {
    let mut found: Vec<RunLedger> = Vec::new();
    let own_id = own.subject.reference.strip_prefix("run:").unwrap_or("");
    let mut runs = state.runs().list();
    runs.reverse();
    for info in runs.iter().filter(|r| r.id != own_id && r.kpis.is_some()) {
        if let Some(handle) = state.runs().get(&info.id)
            && let Ok(l) =
                ledger::from_events(&format!("run:{}", info.id), &handle.journal().since(None))
        {
            found.push(l);
        }
    }
    if let Some(dir) = state.runs().config.replays.clone() {
        let skip = own.subject.replay_of.clone();
        let recorded = tokio::task::spawn_blocking(move || {
            crate::replay::list(&dir)
                .into_iter()
                .filter(|r| r.kpis.is_some() && Some(&r.name) != skip.as_ref())
                .filter_map(|r| ledger::of_record(&dir, &r.name).ok())
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        found.extend(recorded);
    }
    found.retain(|l| ledger::window_mismatch(&own.subject, &l.subject).is_none());
    found
}

/// The facts for run `id` from its `events`, or `None` when it has no ledger (no KPIs yet).
pub(crate) async fn facts(state: &AppState, id: &str, events: &[Envelope]) -> Option<Value> {
    let own = ledger::from_events(&format!("run:{id}"), events).ok()?;
    let others = same_window(state, &own).await;
    let compare = |pick: &dyn Fn(&RunLedger) -> bool| {
        others
            .iter()
            .find(|l| pick(l))
            .and_then(|a| ledger::compare(a.clone(), own.clone()).ok())
    };
    let baseline = (!own.subject.baseline)
        .then(|| compare(&|l| l.subject.baseline))
        .flatten();
    let healthy = own.subject.fault.as_ref().and_then(|_| {
        compare(&|l| {
            !l.subject.baseline
                && l.subject.fault.is_none()
                && l.subject.requests == own.subject.requests
                && l.subject.pressure == own.subject.pressure
                && l.subject.drafts == own.subject.drafts
        })
    });
    let fault = own.subject.fault.as_ref().map(|f| {
        let mode = match f.mode {
            FaultMode::Nan => "replaced with NaN".to_owned(),
            FaultMode::Hold { value } => format!("held at {value}"),
            FaultMode::Bias { offset } => format!("biased by {offset:+}"),
        };
        json!({"point": f.point, "reading": mode, "from_step": f.from_step, "steps": f.steps})
    });
    Some(json!({
        "basis": "BOPTEST simulation of this test case with its own emission factors and constant \
            prices; HVAC energy only (no lighting, plug loads or Scope 3)",
        "this_run": totals(&own),
        "vs_baseline": baseline.as_ref().map(change),
        "test_fault": fault,
        "vs_same_run_without_the_fault": healthy.as_ref().map(change),
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::path::Path;

    use crate::runs::{PlantSource, RunsConfig};

    #[tokio::test]
    async fn a_run_gets_its_totals_and_its_change_against_a_same_window_baseline() {
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
        let events = crate::replay::read(&replays, "day143-morning-your-sequence-rev-b").unwrap();
        let f = super::facts(&state, "x", &events).await.unwrap();
        assert_eq!(f["this_run"]["scope1_gas"]["kg_co2e"], 796.0);
        assert_eq!(f["this_run"]["scope2_electricity"]["kg_co2e"], 2138.0);
        let vs = &f["vs_baseline"];
        assert_eq!(vs["compared_with"], "replay:day143-morning-baseline");
        let change = &vs["change_negative_is_less"];
        assert_eq!(change["scope1_gas"]["kg_co2e"], -848.0);
        assert_eq!(change["total"]["usd"], -269.0);
        assert!(change["thermal_discomfort_kh_per_zone"].as_f64().unwrap() < 0.0);
        assert!(
            f["vs_same_run_without_the_fault"].is_null(),
            "no fault, no comparator"
        );
        assert!(super::facts(&state, "x", &[]).await.is_none());
    }
}
