import type { LedgerComparison } from "@/types/generated/LedgerComparison";
import type { RunLedger } from "@/types/generated/RunLedger";

import { type Resource, useResource } from "./useResource";

/** A run's Scope 1 and 2 ledger, once `enabled` (the run has reported BOPTEST's KPIs). */
export function useRunLedger(runId: string, enabled: boolean): Resource<RunLedger> {
  return useResource<RunLedger>(enabled ? `/api/runs/${encodeURIComponent(runId)}/ledger` : null);
}

/**
 * `b` against `a` (`run:<id>` or `replay:<record>`), or nothing while either is null. The
 * server refuses runs of different windows; that shows as an error the caller may ignore.
 */
export function useLedgerComparison(a: string | null, b: string | null): Resource<LedgerComparison> | null {
  const path =
    a !== null && b !== null ? `/api/ledger/compare?a=${encodeURIComponent(a)}&b=${encodeURIComponent(b)}` : null;
  const r = useResource<LedgerComparison>(path);
  return path === null ? null : r;
}
