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
};

/**
 * The baseline to set beside a run: one with the same start and the same number of steps that
 * reported KPIs. A run on this server (newest first) wins over a recorded one. None for a
 * baseline run itself.
 */
export function findBaseline(cfg: RunConfig, runs: RunInfo[], replays: ReplayInfo[]): BaselineKpis | null {
  if (cfg.baseline) return null;
  const run = [...runs]
    .reverse()
    .find(
      (r) =>
        r.config.baseline &&
        !r.config.replay &&
        r.config.start_s === cfg.start_s &&
        r.config.steps === cfg.steps &&
        r.config.step_s === cfg.step_s &&
        r.config.case === cfg.case &&
        r.kpis !== undefined,
    );
  if (run?.kpis) return { source: `baseline run ${run.id}`, recorded: false, kpis: run.kpis };
  // Replays carry no step length; every record so far uses the runtime's 120 s step.
  const replay = replays.find(
    (r) => r.baseline && r.start_s === cfg.start_s && r.ticks === cfg.steps && cfg.step_s === 120 && r.kpis !== undefined,
  );
  if (replay?.kpis) return { source: `recorded baseline run ${replay.recorded_run}`, recorded: true, kpis: replay.kpis };
  return null;
}

/** Percent change from `base` to `value`; null when the base is zero or either is missing. */
export function change(value: number | undefined, base: number | undefined): number | null {
  if (value === undefined || base === undefined || base === 0) return null;
  return ((value - base) / Math.abs(base)) * 100;
}
