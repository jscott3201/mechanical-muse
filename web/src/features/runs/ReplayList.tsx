import { History } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAction, useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { ReplayRequest } from "@/types/generated/ReplayRequest";
import type { RunInfo } from "@/types/generated/RunInfo";

import { ErrorNote } from "../ErrorNote";
import { byWindow, describe, replayFacts } from "./describe";
import { formatSimTime } from "./trends";

function ReplayButton({
  replay,
  highlighted,
  onStarted,
}: {
  replay: ReplayInfo;
  highlighted: boolean;
  onStarted: (run: RunInfo) => void;
}) {
  const body = useMemo<ReplayRequest>(() => ({ record: replay.name }), [replay.name]);
  const start = useAction<RunInfo>("/api/runs/replay", body);
  const reported = useRef<string | null>(null);
  useEffect(() => {
    if (start.state.status === "done" && reported.current !== start.state.data.id) {
      reported.current = start.state.data.id;
      onStarted(start.state.data);
    }
  }, [start.state, onStarted]);
  // A link to a record points it out; it is still the viewer who starts it.
  const item = useRef<HTMLLIElement>(null);
  useEffect(() => {
    if (highlighted) item.current?.scrollIntoView?.({ block: "center" });
  }, [highlighted]);
  return (
    <li
      ref={item}
      className={cn("flex flex-col gap-1 rounded-md", highlighted && "bg-muted p-2 ring-2 ring-ring")}
      aria-current={highlighted ? "true" : undefined}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="min-w-0 text-sm">
          <span className="block">{describe(replayFacts(replay))}</span>
          <span className="block truncate font-mono text-xs text-muted-foreground">{replay.name}</span>
          <span className="text-xs text-muted-foreground">
            {replay.ticks} steps from {formatSimTime(replay.start_s)}
          </span>
        </span>
        <Button
          size="sm"
          variant="outline"
          onClick={start.run}
          disabled={start.state.status === "running"}
          aria-label={`Replay ${replay.name}`}
        >
          <History aria-hidden="true" /> Replay
        </Button>
      </div>
      {start.state.status === "error" && <ErrorNote error={start.state.error} />}
    </li>
  );
}

/** Recorded runs the server can play back through the same live view, grouped by window. */
export function ReplayList({ highlight, onStarted }: { highlight: string | null; onStarted: (run: RunInfo) => void }) {
  const replays = useResource<ReplayInfo[]>("/api/replays");
  if (replays.status !== "ready" || replays.data.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Replay a recorded run</CardTitle>
        <CardDescription>
          Sends a recorded run's events again through the same view, labelled as a replay. Nothing
          runs against BOPTEST.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {byWindow(replays.data).map((group) => (
          <section key={group.start_s} aria-label={group.label} className="flex flex-col gap-2">
            <h3 className="text-xs font-medium text-muted-foreground">{group.label}</h3>
            <ul className="flex flex-col gap-2">
              {group.items.map((r) => (
                <ReplayButton key={r.name} replay={r} highlighted={r.name === highlight} onStarted={onStarted} />
              ))}
            </ul>
          </section>
        ))}
      </CardContent>
    </Card>
  );
}
