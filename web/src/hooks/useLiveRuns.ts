import { useHealth } from "./useHealth";

/** Shown wherever a live run would start on a server without BOPTEST. */
export const BOPTEST_OFF_TEXT =
  "No BOPTEST configured on this server (set MM_BOPTEST_URL), so live runs are off. Replays still work.";

/**
 * Whether this server can start a live BOPTEST run. While health is loading or unreachable the
 * answer is "yes": the server refuses a run it cannot start with the same plain reason.
 */
export function useLiveRuns(): { available: boolean; reason: string | null } {
  const health = useHealth();
  // Only an explicit "no" turns live runs off; an older server without the field still has them.
  const available = health.status !== "ready" || health.data.boptest !== false;
  return { available, reason: available ? null : BOPTEST_OFF_TEXT };
}
