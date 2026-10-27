import type { Envelope } from "@/types/generated/Envelope";

import { initialRunStream, reduceRun, type RunStreamState } from "./runStream";
import { type Connection, useEventStream } from "./useEventStream";

const KINDS = ["phase", "started", "tick", "kpi", "ended"] as const;

/** Follows one run's event stream (see `useEventStream`). */
export function useRunStream(runId: string | null): RunStreamState & { connection: Connection } {
  return useEventStream<Envelope, RunStreamState>(
    runId === null ? null : `/api/runs/${encodeURIComponent(runId)}/events`,
    KINDS,
    reduceRun,
    initialRunStream,
  );
}
