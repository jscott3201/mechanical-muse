import { Activity, CircleCheck, CircleStop, CircleX, LoaderCircle, Unplug, Clock } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type { RunStreamState } from "@/hooks/runStream";

export type Connection = "idle" | "connecting" | "open" | "reconnecting" | "closed";

/** What the run is doing, always as icon plus text. `stale` means no event for a while. */
export function RunStatus({ stream, connection, stale }: { stream: RunStreamState; connection: Connection; stale: boolean }) {
  const ended = stream.ended;
  if (ended?.state === "completed")
    return <Badge><CircleCheck aria-hidden="true" /> completed</Badge>;
  if (ended?.state === "stopped")
    return <Badge variant="secondary"><CircleStop aria-hidden="true" /> stopped</Badge>;
  if (ended?.state === "failed")
    return <Badge variant="destructive"><CircleX aria-hidden="true" /> failed</Badge>;
  if (connection === "reconnecting")
    return <Badge variant="destructive"><Unplug aria-hidden="true" /> reconnecting</Badge>;
  if (stream.phase === "running")
    return stale ? (
      <Badge variant="destructive"><Clock aria-hidden="true" /> stale: no update for over a minute</Badge>
    ) : (
      <Badge><Activity aria-hidden="true" /> live</Badge>
    );
  return (
    <Badge variant="secondary">
      <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" /> {stream.phase ?? "connecting"}
    </Badge>
  );
}
