import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Amounts } from "@/types/generated/Amounts";
import type { EndUseView } from "@/types/generated/EndUseView";
import type { LedgerComparison } from "@/types/generated/LedgerComparison";
import type { RunLedger } from "@/types/generated/RunLedger";

import { EmissionsPanel, RunEmissions } from "./EmissionsPanel";
import { faultRequest } from "./faults";

const amounts = (kwh: number, kg: number, cost: number): Amounts => ({ kwh, kg_co2e: kg, cost });

/** The day-143 numbers the server computes from the committed replays (scope totals only). */
function ledger(over: Partial<RunLedger>): RunLedger {
  return {
    subject: {
      reference: "run:run-1",
      case: "multizone_office_complex_air",
      start_s: 12376800,
      steps: 240,
      step_s: 120,
      ticks: 240,
      baseline: false,
      requests: true,
      pressure: false,
      drafts: {},
    },
    factors: {
      electricity_kg_per_kwh: 0.341,
      gas_kg_per_kwh: 0.18108,
      electricity_price_per_kwh: 0.094,
      gas_price_per_kwh: 0.024,
      currency: "USD",
      area_m2: 42757,
      source: "BOPTEST",
    },
    scope1: amounts(4395, 795.9, 105.5),
    scope2: amounts(6271, 2138.4, 589.5),
    total: amounts(10666, 2934.3, 694.97),
    boptest_cost: 694.97,
    average_price_per_kwh: 0.05,
    reconciles: true,
    measured: false,
    missing: ["floor1_reaAHU_PFanSup_y", "reaHotWatSys_reaPBoi_y"],
    end_uses: [],
    tdis_tot: 0.1065,
    ...over,
  };
}

afterEach(cleanup);

describe("EmissionsPanel", () => {
  it("shows scope cards, the change against the baseline, comfort and the footer for an old record", () => {
    const base = ledger({ scope1: amounts(9079, 1644, 217.9), scope2: amounts(7932, 2705, 745.6), total: amounts(17011, 4349, 963.53), tdis_tot: 0.1192 });
    const b = ledger({});
    const vs: LedgerComparison = {
      a: base,
      b,
      delta: {
        scope1: amounts(-4684, -848.1, -112.4),
        scope2: amounts(-1661, -566.6, -156.1),
        total: amounts(-6345, -1414.7, -268.56),
        end_uses: [],
        tdis_tot: -0.0127,
      },
    };
    render(<EmissionsPanel ledger={b} vsBaseline={vs} baselineSource="recorded baseline run run-b" fault={undefined} vsHealthy={null} healthySource={null} />);
    expect(screen.getByText("Energy & emissions")).toBeTruthy();
    expect(screen.getByText("796 kgCO₂e")).toBeTruthy();
    expect(screen.getByText("2,138 kgCO₂e")).toBeTruthy();
    expect(screen.getByText(/vs baseline: −848 kg \(−51.6 %\)/)).toBeTruthy();
    expect(screen.getByText("vs baseline: −$269")).toBeTruthy();
    expect(screen.getByText(/more comfortable/)).toBeTruthy();
    expect(screen.getByText(/End uses not recorded/)).toBeTruthy();
    expect(screen.getByText(/Simulation\. Factors and prices are BOPTEST's for this test case/)).toBeTruthy();
    expect(screen.getByText(/lighting, plug loads and Scope 3 are not included/)).toBeTruthy();
    expect(screen.queryByText(/Injected test fault/)).toBeNull();
  });

  it("lists end uses with the unattributed rest and prices a fault against the healthy run", () => {
    const row = (id: string, label: string, scope: number, kwh: number, measured = true): EndUseView => ({
      id,
      label,
      scope,
      amounts: amounts(kwh, kwh * (scope === 1 ? 0.18108 : 0.341), kwh * (scope === 1 ? 0.024 : 0.094)),
      measured,
    });
    const l = ledger({
      measured: true,
      missing: [],
      end_uses: [row("chillers", "Chillers", 2, 3000), row("boiler", "Boiler (gas)", 1, 4300), row("unattributed_gas", "Gas not in the 120 s samples", 1, 95, false)],
      subject: { ...ledger({}).subject, fault: faultRequest("hold", 30) },
    });
    const healthy: LedgerComparison = {
      a: ledger({}),
      b: l,
      delta: { scope1: amounts(0, 0, 0), scope2: amounts(412.3, 140.6, 38.8), total: amounts(412.3, 140.6, 38.8), end_uses: [] },
    };
    render(<EmissionsPanel ledger={l} vsBaseline={null} baselineSource={null} fault={faultRequest("hold", 30)} vsHealthy={healthy} healthySource="run run-3" />);
    const table = screen.getByRole("table", { name: "End uses" });
    expect(within(table).getByText("Chillers")).toBeTruthy();
    expect(within(table).getByText("4,300")).toBeTruthy();
    expect(within(table).getByText("Gas not in the 120 s samples")).toBeTruthy();
    expect(
      screen.getByText(
        "floor1_reaAHU_yCooVal_y held at 1 from step 30: +412 kWh, +$39, +141 kgCO₂e vs run run-3",
      ),
    ).toBeTruthy();
  });

  it("labels the fault without a cost when no healthy run of the window exists, and flags a split that does not reconcile", () => {
    render(
      <EmissionsPanel ledger={ledger({ reconciles: false })} vsBaseline={null} baselineSource={null} fault={faultRequest("nan", 20)} vsHealthy={null} healthySource={null} />,
    );
    expect(screen.getByText("floor1_reaAHU_TSup_y reads NaN from step 20.")).toBeTruthy();
    expect(screen.getByText(/does not re-price to BOPTEST's/)).toBeTruthy();
  });
});

describe("RunEmissions", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("says why there is no ledger instead of showing nothing", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ code: "no-kpis", message: "the run has no KPIs yet" }), { status: 409 })),
    );
    render(<RunEmissions runId="run-1" hasKpis fault={undefined} comparisons={{ baseline: null, healthy: null }} />);
    expect(await screen.findByText(/No emissions ledger for this run/)).toBeTruthy();
    expect(screen.getByText("no-kpis")).toBeTruthy();
    expect(screen.getByText(/the run has no KPIs yet/)).toBeTruthy();
  });
});

describe("EmissionsPanel for the home", () => {
  it("shows Scope 2 only, euros, the average price paid and the house's factor", () => {
    const home = (kwh: number, cost: number): RunLedger =>
      ledger({
        subject: { ...ledger({}).subject, case: "bestest_hydronic_heat_pump", step_s: 900 },
        factors: { electricity_kg_per_kwh: 0.167, currency: "EUR", area_m2: 192, source: "BOPTEST" },
        scope1: amounts(0, 0, 0),
        scope2: amounts(kwh, kwh * 0.167, cost),
        total: amounts(kwh, kwh * 0.167, cost),
        boptest_cost: cost,
        average_price_per_kwh: cost / kwh,
      });
    const base = home(340.4, 79.21);
    const b = home(314.3, 72.59);
    const vs: LedgerComparison = {
      a: base,
      b,
      delta: {
        scope1: amounts(0, 0, 0),
        scope2: amounts(-26.1, -4.36, -6.62),
        total: amounts(-26.1, -4.36, -6.62),
        end_uses: [],
      },
    };
    render(<EmissionsPanel ledger={b} vsBaseline={vs} baselineSource="recorded baseline run run-b" fault={undefined} vsHealthy={null} healthySource={null} />);
    expect(screen.getByText(/All electric: Scope 2/)).toBeTruthy();
    expect(screen.getByText("None")).toBeTruthy();
    expect(screen.getByText(/No fuel burnt on site/)).toBeTruthy();
    expect(screen.getByText("52.5 kgCO₂e")).toBeTruthy();
    expect(screen.getByText("€72.6")).toBeTruthy();
    expect(screen.getByText("vs baseline: −€6.6")).toBeTruthy();
    expect(screen.getByText(/Average price paid €0\.231\/kWh \(baseline €0\.2327\)/)).toBeTruthy();
    expect(screen.getByText(/0\.167 kgCO₂\/kWh, the same every hour; spot electricity prices/)).toBeTruthy();
    expect(screen.queryByText(/\$/)).toBeNull();
  });
});

