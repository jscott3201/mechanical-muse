import { formatNumber } from "@/lib/format";
import type { PeakPower } from "@/types/generated/PeakPower";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunInfo } from "@/types/generated/RunInfo";

/** KPIs of a baseline run (BOPTEST's own controllers, nothing written) of the same window. */
export type BaselineKpis = {
  /** Where they come from, for the table caption. */
  source: string;
  /** True when they come from a recorded run rather than one run on this server. */
  recorded: boolean;
  kpis: Record<string, number>;
  /** The run as the ledger routes name it: `run:<id>` or `replay:<record>`. */
  reference: string;
  /** A home baseline's power in the window's most expensive quarter of prices. */
  peak?: PeakPower;
};

/** Another run of the same window to compare with: how the ledger routes name it, and a caption. */
export type Comparator = {
  reference: string;
  source: string;
  /** Its KPIs as BOPTEST reported them, when it has any. */
  kpis?: Record<string, number>;
};

function sameRunWindow(r: RunInfo, cfg: RunConfig): boolean {
  return (
    r.config.start_s === cfg.start_s &&
    r.config.steps === cfg.steps &&
    r.config.step_s === cfg.step_s &&
    r.config.case === cfg.case &&
    r.kpis !== undefined
  );
}

function sameReplayWindow(r: ReplayInfo, cfg: RunConfig): boolean {
  return (
    r.start_s === cfg.start_s &&
    r.ticks === cfg.steps &&
    r.step_s === cfg.step_s &&
    r.case === cfg.case &&
    r.kpis !== undefined
  );
}

/**
 * The baseline to set beside a run: one with the same start and the same number of steps that
 * reported KPIs. A run on this server (newest first) wins over a recorded one. None for a
 * baseline run itself.
 */
export function findBaseline(cfg: RunConfig, runs: RunInfo[], replays: ReplayInfo[]): BaselineKpis | null {
  if (cfg.baseline) return null;
  const run = [...runs].reverse().find((r) => r.config.baseline && !r.config.replay && sameRunWindow(r, cfg));
  if (run?.kpis)
    return {
      source: `baseline run ${run.id}`,
      recorded: false,
      kpis: run.kpis,
      reference: `run:${run.id}`,
      ...(run.peak ? { peak: run.peak } : {}),
    };
  const replay = replays.find((r) => r.baseline && sameReplayWindow(r, cfg));
  if (replay?.kpis)
    return {
      source: `recorded baseline run ${replay.recorded_run}`,
      recorded: true,
      kpis: replay.kpis,
      reference: `replay:${replay.name}`,
      ...(replay.peak ? { peak: replay.peak } : {}),
    };
  return null;
}

/** Same specs drafted, from the same graphs (content ids), not only the same spec names. */
function sameDrafts(a: Partial<Record<string, string>>, b: Partial<Record<string, string>>): boolean {
  const x = Object.keys(a).sort();
  const y = Object.keys(b).sort();
  return x.length === y.length && x.every((k, i) => k === y[i] && a[k] === b[k]);
}

/**
 * For a run with a test fault: the same run without it (same window, same switches and drafts,
 * no fault, not a baseline). None for a run without a fault.
 */
export function findHealthy(cfg: RunConfig, runs: RunInfo[], replays: ReplayInfo[]): Comparator | null {
  if (!cfg.fault || cfg.baseline) return null;
  const drafts: Partial<Record<string, string>> = cfg.drafts ?? {};
  const run = [...runs]
    .reverse()
    .find(
      (r) =>
        !r.config.baseline &&
        !r.config.fault &&
        !r.config.replay &&
        r.config.requests === cfg.requests &&
        (r.config.pressure ?? false) === (cfg.pressure ?? false) &&
        sameDrafts(r.config.drafts ?? {}, drafts) &&
        sameRunWindow(r, cfg),
    );
  if (run) return { reference: `run:${run.id}`, source: `run ${run.id}`, ...(run.kpis ? { kpis: run.kpis } : {}) };
  const replay = replays.find(
    (r) =>
      !r.baseline &&
      !r.fault &&
      r.requests === cfg.requests &&
      r.pressure === (cfg.pressure ?? false) &&
      sameDrafts(r.draft_ids ?? {}, drafts) &&
      sameReplayWindow(r, cfg),
  );
  if (replay)
    return {
      reference: `replay:${replay.name}`,
      source: `recorded run ${replay.recorded_run}`,
      ...(replay.kpis ? { kpis: replay.kpis } : {}),
    };
  return null;
}

/** Percent change from `base` to `value`; null when the base is zero or either is missing. */
export function change(value: number | undefined, base: number | undefined): number | null {
  if (value === undefined || base === undefined || base === 0) return null;
  return ((value - base) / Math.abs(base)) * 100;
}

/** A percent change as text ("+1.2 %", "−3 %"); one that rounds to zero reads "0 %", never "−0 %". */
export function formatChange(pct: number): string {
  const text = formatNumber(Math.abs(pct), 1);
  return `${text === "0" ? "" : pct > 0 ? "+" : "−"}${text} %`;
}
