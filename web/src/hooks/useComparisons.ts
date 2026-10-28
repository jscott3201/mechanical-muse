import { useMemo } from "react";

import { type BaselineKpis, type Comparator, findBaseline, findHealthy } from "@/features/runs/baseline";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunInfo } from "@/types/generated/RunInfo";

import { useResource } from "./useResource";

export type Comparisons = {
  /** A baseline run of the same window (BOPTEST's own controllers). */
  baseline: BaselineKpis | null;
  /** For a run with a test fault, the same run without it. */
  healthy: Comparator | null;
};

const NONE: Comparisons = { baseline: null, healthy: null };

/**
 * The runs to set beside `cfg` once it has KPIs to compare (`enabled`). Fetches the run and
 * replay lists then; a server with replays off has none.
 */
export function useComparisons(cfg: RunConfig | null, enabled: boolean): Comparisons {
  const wanted = enabled && cfg !== null && !cfg.baseline;
  const runs = useResource<RunInfo[]>(wanted ? "/api/runs" : null);
  const replays = useResource<ReplayInfo[]>(wanted ? "/api/replays" : null);
  return useMemo(() => {
    if (!wanted || cfg === null) return NONE;
    const r = runs.status === "ready" ? runs.data : [];
    const p = replays.status === "ready" ? replays.data : [];
    return { baseline: findBaseline(cfg, r, p), healthy: findHealthy(cfg, r, p) };
  }, [wanted, cfg, runs, replays]);
}
