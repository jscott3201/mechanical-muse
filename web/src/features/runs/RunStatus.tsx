import {
  Activity,
  CircleCheck,
  CircleStop,
  CircleX,
  Clock,
  LoaderCircle,
  ShieldAlert,
  Unplug,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type { RunStreamState } from "@/hooks/runStream";

export type Connection = "idle" | "connecting" | "open" | "reconnecting" | "closed";

/**
 * What the run is doing, always as icon plus text. `staleS` is the seconds since the last event
 * while running, or null when updates are current.
 */
export function RunStatus({
  stream,
  connection,
  staleS,
}: {
  stream: RunStreamState;
  connection: Connection;
  staleS: number | null;
}) {
  const ended = stream.ended;
  if (ended?.state === "completed")
    return <Badge><CircleCheck aria-hidden="true" /> completed</Badge>;
  if (ended?.state === "stopped")
    return <Badge variant="secondary"><CircleStop aria-hidden="true" /> stopped</Badge>;
  if (ended?.state === "failed")
    return <Badge variant="destructive"><CircleX aria-hidden="true" /> failed</Badge>;
  if (connection === "reconnecting")
    return <Badge variant="destructive"><Unplug aria-hidden="true" /> reconnecting</Badge>;
  if (connection === "closed")
    return <Badge variant="destructive"><Unplug aria-hidden="true" /> disconnected</Badge>;
  if (stream.phase === "running") {
    if (staleS !== null)
      return <Badge variant="destructive"><Clock aria-hidden="true" /> stale: no update for {staleS} s</Badge>;
    if (stream.degraded !== null)
      return <Badge variant="destructive"><ShieldAlert aria-hidden="true" /> degraded: {stream.degraded}</Badge>;
    return <Badge><Activity aria-hidden="true" /> live</Badge>;
  }
  return (
    <Badge variant="secondary">
      <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" /> {stream.phase ?? "connecting"}
    </Badge>
  );
}
