import type { Fault } from "@/types/generated/Fault";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";

import { faultModeText } from "./faults";
import { formatSimTime } from "./trends";

const PLANT = "chw_plant_reset";
const HOME_SPEC = "home_price_response";
/** BOPTEST's home test case (a heat pump house on spot prices). */
export const HOME_CASE = "bestest_hydronic_heat_pump";

/** What a run or recorded run ran, in the terms the labels need. */
export type RunFacts = {
  /** BOPTEST's home (heat pump house) rather than the office. */
  home: boolean;
  baseline: boolean;
  pressure: boolean;
  /** Engineers' own sequences, each run from a verified Spark draft. */
  own: string[];
  /** Specs that ran from a drafted graph (own sequences included). */
  drafts: string[];
  /** The labelled test fault the run carried, if any. */
  fault?: Fault | null;
};

export function replayFacts(r: ReplayInfo): RunFacts {
  return { home: r.case === HOME_CASE, baseline: r.baseline, pressure: r.pressure, own: r.own, drafts: r.drafts ?? [], fault: r.fault ?? null };
}

/** Old records may lack `drafts`, `bindings` or `pressure`; treat them as empty. */
export function configFacts(c: RunConfig): RunFacts {
  const drafts: Partial<Record<string, string>> = c.drafts ?? {};
  const bindings: Partial<Record<string, unknown>> = c.bindings ?? {};
  return {
    home: c.case === HOME_CASE,
    baseline: c.baseline,
    pressure: c.pressure ?? false,
    own: Object.keys(bindings).sort(),
    drafts: Object.keys(drafts).sort(),
    fault: c.fault ?? null,
  };
}

/** What ran, in words: never calls a hand-built graph a Spark draft or the reverse, and names a
 * test fault so a faulted run never reads like its healthy twin. */
export function describe(f: RunFacts): string {
  const base = describeGraphs(f);
  return f.fault ? `${base}; test fault: ${f.fault.point} ${faultModeText(f.fault)}` : base;
}

function describeGraphs(f: RunFacts): string {
  if (f.home) {
    if (f.baseline) return "Home: BOPTEST baseline, nothing written";
    return f.drafts.includes(HOME_SPEC) ? "Home price response (Spark draft)" : "Home price response (hand-built)";
  }
  if (f.baseline) return "BOPTEST baseline, nothing written";
  const pressure = f.pressure ? " and static pressure" : "";
  const ours = f.drafts.filter((s) => !f.own.includes(s));
  const plantDrafted = ours.includes(PLANT);
  if (f.own.length > 0) {
    const plant = plantDrafted ? "plant reset Spark-drafted" : "plant reset hand-built";
    return `Your sequence ${f.own.join(", ")} (Spark draft); ${plant}${pressure ? ", with static pressure" : ""}`;
  }
  const graphs = ours.length === 0 ? "Hand-built sequences" : plantDrafted && ours.length === 1 ? "Spark-drafted plant reset" : "Spark-drafted graphs";
  return `${graphs}, chilled water${pressure}`;
}

/** Recorded runs grouped by their simulated window, in time order. */
export function byWindow<T extends { start_s: number }>(items: T[]): { label: string; start_s: number; items: T[] }[] {
  const groups = new Map<number, T[]>();
  for (const it of items) groups.set(it.start_s, [...(groups.get(it.start_s) ?? []), it]);
  return [...groups.entries()]
    .sort(([a], [b]) => a - b)
    .map(([start_s, list]) => ({ label: windowLabel(start_s), start_s, items: list }));
}

/** "Day 185 noon" style for the usual starts; the clock time otherwise. */
export function windowLabel(start_s: number): string {
  const day = Math.floor(start_s / 86400);
  const hour = (start_s - day * 86400) / 3600;
  const part = hour === 0 ? "midnight" : hour === 12 ? "noon" : hour >= 5 && hour < 12 ? "morning" : null;
  return part ? `Day ${day} ${part}` : formatSimTime(start_s).replace(/^day/, "Day");
}
