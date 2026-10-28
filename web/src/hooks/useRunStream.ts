import type { Envelope } from "@/types/generated/Envelope";

import { initialRunStream, reduceRun, type RunStreamState } from "./runStream";
import { type Connection, useEventStream } from "./useEventStream";

/**
 * Every event name the server sends. An `EventSource` only delivers names it listens for, so a
 * kind missing here is dropped and shows as a gap; the record below makes the compiler list
 * each kind of the generated union exactly once.
 */
const KIND_SET: Record<Envelope["kind"], true> = {
  phase: true,
  started: true,
  tick: true,
  kpi: true,
  "peak-power": true,
  ended: true,
};
export const KINDS = Object.keys(KIND_SET) as Envelope["kind"][];

/** Follows one run's event stream (see `useEventStream`). */
export function useRunStream(runId: string | null): RunStreamState & { connection: Connection } {
  return useEventStream<Envelope, RunStreamState>(
    runId === null ? null : `/api/runs/${encodeURIComponent(runId)}/events`,
    KINDS,
    reduceRun,
    initialRunStream,
  );
}
