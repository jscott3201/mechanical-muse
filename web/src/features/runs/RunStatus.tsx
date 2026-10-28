import {
  Activity,
  CircleCheck,
  CircleStop,
  CircleX,
  Clock,
  History,
  LoaderCircle,
  ShieldAlert,
  Unplug,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type { RunStreamState } from "@/hooks/runStream";
import type { Connection } from "@/hooks/useEventStream";

type StatusProps = {
  stream: RunStreamState;
  connection: Connection;
  staleS: number | null;
};

/**
 * What the run is doing, always as icon plus text. `staleS` is the seconds since the last event
 * while running, or null when updates are current. Changes are announced politely.
 */
export function RunStatus(props: StatusProps) {
  return (
    <span role="status" aria-live="polite" className="inline-flex">
      <StatusBadge {...props} />
    </span>
  );
}

function StatusBadge({ stream, connection, staleS }: StatusProps) {
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
    // A replay streams recorded events: never call it live.
    return stream.config?.replay ? (
      <Badge variant="secondary"><History aria-hidden="true" /> replaying</Badge>
    ) : (
      <Badge><Activity aria-hidden="true" /> live</Badge>
    );
  }
  return (
    <Badge variant="secondary">
      <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" /> {stream.phase ?? "connecting"}
    </Badge>
  );
}
