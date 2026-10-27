import { describe, expect, it } from "vitest";

import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunInfo } from "@/types/generated/RunInfo";

import { change, findBaseline } from "./baseline";

const cfg: RunConfig = {
  baseline: false,
  requests: true,
  pressure: false,
  start_s: 16027200,
  warmup_s: 86400,
  steps: 300,
  step_s: 120,
  case: "multizone_office_complex_air",
  drafts: {},
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
  start_s: 16027200,
  drafted: false,
  pressure: false,
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
    });
  });

  it("falls back to a recorded baseline of the same window", () => {
    const found = findBaseline(cfg, [run("run-1", { baseline: true })], [replay({ ticks: 60 }), replay({})]);
    expect(found).toEqual({ source: "recorded baseline run run-1", recorded: true, kpis: { ener_tot: 0.3255 } });
  });

  it("finds nothing for another window, a non-baseline run, or a baseline run itself", () => {
    expect(findBaseline(cfg, [run("run-1", { steps: 60, baseline: true }, { ener_tot: 1 })], [replay({ start_s: 1 })])).toBeNull();
    expect(findBaseline(cfg, [run("run-1", {}, { ener_tot: 1 })], [replay({ baseline: false })])).toBeNull();
    expect(findBaseline({ ...cfg, baseline: true }, [], [replay({})])).toBeNull();
  });
});

describe("change", () => {
  it("is the percent change from the base", () => {
    expect(change(0.3101, 0.3255)).toBeCloseTo(-4.73, 2);
    expect(change(1, 0)).toBeNull();
    expect(change(undefined, 1)).toBeNull();
  });
});
