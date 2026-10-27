import { Play } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useAction, useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import type { RunInfo } from "@/types/generated/RunInfo";
import type { RunRequest } from "@/types/generated/RunRequest";

import { ErrorNote } from "../ErrorNote";
import { RunView } from "./RunView";
import { formatSimTime } from "./trends";

/** Day 185 is the Monday after the case's peak cooling day; noon is when the plant is busy. */
const DEFAULTS = { day: 185, hour: 12, steps: 300 };

function StartForm({ onStarted }: { onStarted: (run: RunInfo) => void }) {
  const [baseline, setBaseline] = useState(false);
  const [requests, setRequests] = useState(true);
  const [day, setDay] = useState(DEFAULTS.day);
  const [hour, setHour] = useState(DEFAULTS.hour);
  const [steps, setSteps] = useState(DEFAULTS.steps);
  const body = useMemo<RunRequest>(
    () => ({ baseline, requests, steps, start_s: day * 86400 + hour * 3600 }),
    [baseline, requests, steps, day, hour],
  );
  const start = useAction<RunInfo>("/api/runs", body);
  useEffect(() => {
    if (start.state.status === "done") onStarted(start.state.data);
  }, [start.state, onStarted]);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Start a run</CardTitle>
        <CardDescription>
          On the BOPTEST large-office emulator, with a one-day warm-up. Each run re-verifies its
          sequences first; one run at a time.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <Label htmlFor="run-baseline">Baseline only (write nothing)</Label>
          <Switch id="run-baseline" checked={baseline} onCheckedChange={setBaseline} />
        </div>
        <div className="flex items-center justify-between gap-2">
          <Label htmlFor="run-requests">Air handler requests</Label>
          <Switch id="run-requests" checked={requests && !baseline} disabled={baseline} onCheckedChange={setRequests} />
        </div>
        <div className="grid grid-cols-3 gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="run-day">Day</Label>
            <Input id="run-day" type="number" min={1} max={364} value={day} onChange={(e) => setDay(Number(e.target.value))} />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="run-hour">Hour</Label>
            <Input id="run-hour" type="number" min={0} max={23} value={hour} onChange={(e) => setHour(Number(e.target.value))} />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="run-steps">Steps</Label>
            <Input id="run-steps" type="number" min={1} max={5040} value={steps} onChange={(e) => setSteps(Number(e.target.value))} />
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {steps} steps of 120 s = {(steps * 120) / 3600} simulated hours from {formatSimTime(day * 86400 + hour * 3600)}.
        </p>
        <Button onClick={start.run} disabled={start.state.status === "running"}>
          <Play aria-hidden="true" /> {start.state.status === "running" ? "Starting…" : "Start"}
        </Button>
        {start.state.status === "error" && <ErrorNote error={start.state.error} />}
      </CardContent>
    </Card>
  );
}

export function RunsPage() {
  const runs = useResource<RunInfo[]>("/api/runs");
  const [selected, setSelected] = useState<string | null>(null);
  const list = runs.status === "ready" ? [...runs.data].reverse() : [];
  const current = selected ?? list[0]?.id ?? null;
  const { reload } = runs;
  return (
    <div className="grid gap-4 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <div className="flex flex-col gap-4">
        <StartForm
          onStarted={(run) => {
            setSelected(run.id);
            reload();
          }}
        />
        {runs.status === "error" && <ErrorNote error={runs.error} onRetry={reload} />}
        {list.length > 0 && (
          <nav aria-label="Runs" className="flex flex-col gap-1">
            {list.map((r) => (
              <button
                key={r.id}
                type="button"
                onClick={() => setSelected(r.id)}
                aria-current={r.id === current ? "page" : undefined}
                className={cn("flex flex-col rounded-md px-3 py-2 text-left text-sm hover:bg-muted", r.id === current && "bg-muted")}
              >
                <span className="font-mono text-xs">{r.id}</span>
                <span className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">{r.state}</Badge>
                  {r.config.baseline ? "baseline" : "reset"} · {r.ticks}/{r.config.steps} steps
                </span>
              </button>
            ))}
          </nav>
        )}
      </div>
      <section className="min-w-0">
        {current ? <RunView runId={current} /> : <p className="text-sm text-muted-foreground">No runs yet.</p>}
      </section>
    </div>
  );
}
