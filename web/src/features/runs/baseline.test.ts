import { describe, expect, it } from "vitest";

import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunInfo } from "@/types/generated/RunInfo";

import { change, findBaseline, findHealthy, formatChange } from "./baseline";

const cfg: RunConfig = {
  baseline: false,
  requests: true,
  pressure: false,
  start_s: 16027200,
  warmup_s: 86400,
  steps: 300,
  step_s: 120,
  case: "multizone_office_complex_air",
  drafts: {}, bindings: {},
};

const run = (id: string, over: Partial<RunConfig>, kpis?: Record<string, number>): RunInfo => ({
  id,
  config: { ...cfg, ...over },
  state: "completed",
  ticks: 300,
  events: 307,
  ...(kpis ? { kpis } : {}),
});

const replay = (over: Partial<ReplayInfo>): ReplayInfo => ({
  name: "day185-noon-baseline",
  recorded_run: "run-1",
  ticks: 300,
  baseline: true,
  case: "multizone_office_complex_air",
  step_s: 120,
  start_s: 16027200,
  drafted: false,
  drafts: [],
  pressure: false,
  own: [],
  requests: false,
  draft_ids: {},
  state: "completed",
  kpis: { ener_tot: 0.3255 },
  ...over,
});

describe("findBaseline", () => {
  it("prefers the newest baseline run of the same window on this server", () => {
    const runs = [
      run("run-1", { baseline: true }, { ener_tot: 0.3 }),
      run("run-2", { baseline: true }, { ener_tot: 0.3255 }),
      run("run-3", { baseline: true, start_s: 0 }, { ener_tot: 9 }),
    ];
    expect(findBaseline(cfg, runs, [replay({})])).toEqual({
      source: "baseline run run-2",
      recorded: false,
      kpis: { ener_tot: 0.3255 },
      reference: "run:run-2",
    });
  });

  it("falls back to a recorded baseline of the same window", () => {
    const found = findBaseline(cfg, [run("run-1", { baseline: true })], [replay({ ticks: 60 }), replay({})]);
    expect(found).toEqual({
      source: "recorded baseline run run-1",
      recorded: true,
      kpis: { ener_tot: 0.3255 },
      reference: "replay:day185-noon-baseline",
    });
  });

  it("finds nothing for another window, a non-baseline run, or a baseline run itself", () => {
    expect(findBaseline(cfg, [run("run-1", { steps: 60, baseline: true }, { ener_tot: 1 })], [replay({ start_s: 1 })])).toBeNull();
    expect(findBaseline(cfg, [run("run-1", {}, { ener_tot: 1 })], [replay({ baseline: false })])).toBeNull();
    expect(findBaseline({ ...cfg, baseline: true }, [], [replay({})])).toBeNull();
  });
});

describe("findHealthy", () => {
  const fault = { point: "floor1_reaAHU_yCooVal_y", from_step: 30, steps: 60, mode: { kind: "hold" as const, value: 1 } };
  const faulted: RunConfig = { ...cfg, fault };

  it("finds the same run without the fault, on this server first", () => {
    const runs = [
      run("run-1", {}, { ener_tot: 0.31 }),
      run("run-2", { fault }, { ener_tot: 0.33 }),
      run("run-3", { pressure: true }, { ener_tot: 0.25 }),
    ];
    expect(findHealthy(faulted, runs, [])).toEqual({ reference: "run:run-1", source: "run run-1", kpis: { ener_tot: 0.31 } });
    const recorded = replay({ name: "day185-noon-requests", baseline: false, requests: true });
    expect(findHealthy(faulted, [], [recorded])).toEqual({
      reference: "replay:day185-noon-requests",
      source: "recorded run run-1",
      kpis: { ener_tot: 0.3255 },
    });
  });

  it("finds nothing without a fault, for a baseline, a faulted or another-window run", () => {
    expect(findHealthy(cfg, [run("run-1", {}, { ener_tot: 1 })], [])).toBeNull();
    expect(findHealthy(faulted, [run("run-1", { baseline: true }, { ener_tot: 1 })], [replay({})])).toBeNull();
    expect(findHealthy(faulted, [run("run-1", { steps: 60 }, { ener_tot: 1 })], [])).toBeNull();
    expect(findHealthy(faulted, [], [replay({ baseline: false, requests: true, fault })])).toBeNull();
  });

  it("does not take a run of another draft revision for the healthy one", () => {
    const drafted: RunConfig = { ...faulted, drafts: { chw_plant_reset: "cxf:fnv1a128:aaa" } };
    const other = run("run-1", { drafts: { chw_plant_reset: "cxf:fnv1a128:bbb" } }, { ener_tot: 1 });
    const same = run("run-2", { drafts: { chw_plant_reset: "cxf:fnv1a128:aaa" } }, { ener_tot: 1 });
    expect(findHealthy(drafted, [other], [])).toBeNull();
    expect(findHealthy(drafted, [same, other], [])?.reference).toBe("run:run-2");
    const recorded = replay({ baseline: false, requests: true, drafts: ["chw_plant_reset"], draft_ids: { chw_plant_reset: "cxf:fnv1a128:bbb" } });
    expect(findHealthy(drafted, [], [recorded])).toBeNull();
  });
});

describe("change", () => {
  it("is the percent change from the base", () => {
    expect(change(0.3101, 0.3255)).toBeCloseTo(-4.73, 2);
    expect(change(1, 0)).toBeNull();
    expect(change(undefined, 1)).toBeNull();
  });

  it("writes a change that rounds to zero as 0 %, without a sign", () => {
    expect(formatChange(-4.73)).toBe("−4.7 %");
    expect(formatChange(1.26)).toBe("+1.3 %");
    expect(formatChange(-0.03)).toBe("0 %");
    expect(formatChange(0)).toBe("0 %");
  });
});
