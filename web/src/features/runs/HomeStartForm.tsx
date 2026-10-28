import { Play } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useLiveRuns } from "@/hooks/useLiveRuns";
import { useAction, useResource } from "@/hooks/useResource";
import { draftsOf } from "@/lib/drafts";
import { cn } from "@/lib/utils";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { RunInfo } from "@/types/generated/RunInfo";
import type { RunRequest } from "@/types/generated/RunRequest";

import { ErrorNote } from "../ErrorNote";

const SPEC = "home_price_response";
/** Two weeks of 15-minute steps, the length the home's results were measured over. */
const TWO_WEEKS = 1344;

type Window = { id: string; label: string; detail: string; day: number };

/** BOPTEST's two heating windows for the home, and two the sequence was not tuned on. */
const WINDOWS: readonly Window[] = [
  { id: "typical", label: "Typical heating", detail: "BOPTEST's window from day 108 (April)", day: 108 },
  { id: "peak", label: "Peak heating", detail: "BOPTEST's window from day 16 (January)", day: 16 },
  { id: "feb", label: "February", detail: "From day 40; not tuned on", day: 40 },
  { id: "nov", label: "November", detail: "From day 315; not tuned on", day: 315 },
];

/** Start a run on BOPTEST's home: a heat pump house on spot prices, two weeks per window. */
export function HomeStartForm({ onStarted }: { onStarted: (run: RunInfo) => void }) {
  const [chosen, setChosen] = useState<Window>(WINDOWS[0] as Window);
  const [baseline, setBaseline] = useState(false);
  const stored = useResource<DraftInfo[]>("/api/drafts");
  const drafts = stored.status === "ready" ? draftsOf(stored.data, SPEC) : [];
  const [draft, setDraft] = useState<string | null>(null);
  const body = useMemo<RunRequest>(
    () => ({
      building: "home",
      baseline,
      start_s: chosen.day * 86400,
      steps: TWO_WEEKS,
      ...(draft && !baseline ? { drafts: { [SPEC]: draft } } : {}),
    }),
    [chosen, baseline, draft],
  );
  const start = useAction<RunInfo>("/api/runs", body);
  const live = useLiveRuns();
  const reported = useRef<string | null>(null);
  useEffect(() => {
    if (start.state.status === "done" && reported.current !== start.state.data.id) {
      reported.current = start.state.data.id;
      onStarted(start.state.data);
    }
  }, [start.state, onStarted]);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Start a home run</CardTitle>
        <CardDescription>
          BOPTEST's simulated house in Brussels (heat pump, floor heating) on spot prices: two weeks in 15-minute steps after a
          week of warm-up. The sequence is verified again first.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-col gap-1" role="group" aria-label="Window">
          {WINDOWS.map((w) => (
            <button
              key={w.id}
              type="button"
              onClick={() => setChosen(w)}
              aria-pressed={w.id === chosen.id}
              className={cn(
                "flex flex-col rounded-md border px-3 py-2 text-left text-sm hover:bg-muted focus-visible:outline-2",
                w.id === chosen.id && "border-primary bg-muted",
              )}
            >
              <span className="font-medium">{w.label}</span>
              <span className="text-xs text-muted-foreground">{w.detail}</span>
            </button>
          ))}
        </div>
        <div className="flex items-center justify-between gap-2">
          <Label htmlFor="home-baseline">Baseline only (BOPTEST's own controller)</Label>
          <Switch id="home-baseline" checked={baseline} onCheckedChange={setBaseline} />
        </div>
        {drafts.length > 0 && (
          <div className="flex flex-col gap-1">
            <span className="text-sm">Graph</span>
            <label className="flex items-center gap-2 text-sm">
              <input type="radio" name="home-graph" checked={draft === null} disabled={baseline} onChange={() => setDraft(null)} />
              Hand-built reference
            </label>
            {drafts.slice(0, 3).map((d) => (
              <label key={d.content_id} className="flex items-center gap-2 text-sm">
                <input
                  type="radio"
                  name="home-graph"
                  checked={draft === d.content_id}
                  disabled={baseline}
                  onChange={() => setDraft(d.content_id)}
                />
                Spark draft <span className="font-mono text-xs">{d.content_id.slice(0, 12)}</span>
              </label>
            ))}
          </div>
        )}
        <Button onClick={start.run} disabled={start.state.status === "running" || !live.available}>
          <Play aria-hidden="true" /> {start.state.status === "running" ? "Starting…" : "Start"}
        </Button>
        {live.reason && <p className="text-sm text-muted-foreground">{live.reason}</p>}
        {start.state.status === "error" && <ErrorNote error={start.state.error} />}
      </CardContent>
    </Card>
  );
}
