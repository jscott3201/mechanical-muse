import { useMemo } from "react";

import { type BaselineKpis, findBaseline } from "@/features/runs/baseline";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunInfo } from "@/types/generated/RunInfo";

import { useResource } from "./useResource";

/**
 * The KPIs of a baseline run of the same window as `cfg`, once `cfg` has KPIs to compare
 * (`enabled`). Fetches the run and replay lists then; a server with replays off has none.
 */
export function useBaselineKpis(cfg: RunConfig | null, enabled: boolean): BaselineKpis | null {
  const wanted = enabled && cfg !== null && !cfg.baseline;
  const runs = useResource<RunInfo[]>(wanted ? "/api/runs" : null);
  const replays = useResource<ReplayInfo[]>(wanted ? "/api/replays" : null);
  return useMemo(() => {
    if (!wanted || cfg === null) return null;
    const r = runs.status === "ready" ? runs : null;
    const p = replays.status === "ready" ? replays : null;
    return findBaseline(cfg, r?.data ?? [], p?.data ?? []);
  }, [wanted, cfg, runs, replays]);
}
