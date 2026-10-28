import { History } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

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

/** Replay speeds offered, as multiples of the server's pace (it accepts 1 to 10). */
const REPLAY_SPEEDS = [1, 4, 10] as const;

function ReplayButton({
  replay,
  highlighted,
  onStarted,
  speed,
}: {
  replay: ReplayInfo;
  highlighted: boolean;
  onStarted: (run: RunInfo) => void;
  speed: number;
}) {
  const body = useMemo<ReplayRequest>(
    () => (speed === 1 ? { record: replay.name } : { record: replay.name, speed }),
    [replay.name, speed],
  );
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
          <span className="block font-mono text-xs break-all text-muted-foreground">{replay.name}</span>
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
  const [speed, setSpeed] = useState(1);
  if (replays.status === "loading") return null;
  // A server started without a recordings folder answers 404 `no-replays`: that is an empty
  // list, not a failure. Anything else is shown, so a broken list never looks like an empty one.
  const none = replays.status === "ready" ? replays.data.length === 0 : replays.error.code === "no-replays";
  if (replays.status === "error" && !none) return <ErrorNote error={replays.error} onRetry={replays.reload} />;
  if (none || replays.status !== "ready")
    return (
      <p className="flex items-center gap-2 px-3 text-sm text-muted-foreground">
        <History aria-hidden="true" className="size-4" /> No recorded runs on this server.
      </p>
    );
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
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span id="replay-speed">Speed</span>
          <div className="inline-flex rounded-lg bg-muted p-1" role="group" aria-labelledby="replay-speed">
            {REPLAY_SPEEDS.map((s) => (
              <Button
                key={s}
                variant="ghost"
                size="sm"
                aria-pressed={speed === s}
                onClick={() => setSpeed(s)}
                className={cn("h-7 px-2", speed === s && "bg-background shadow-sm")}
              >
                {s}×
              </Button>
            ))}
          </div>
          <span>Only the wait between steps changes; the recorded events are the same.</span>
        </div>
        {byWindow(replays.data).map((group) => (
          <section key={group.start_s} aria-label={group.label} className="flex flex-col gap-2">
            <h3 className="text-xs font-medium text-muted-foreground">{group.label}</h3>
            <ul className="flex flex-col gap-2">
              {group.items.map((r) => (
                <ReplayButton key={r.name} replay={r} highlighted={r.name === highlight} onStarted={onStarted} speed={speed} />
              ))}
            </ul>
          </section>
        ))}
      </CardContent>
    </Card>
  );
}
