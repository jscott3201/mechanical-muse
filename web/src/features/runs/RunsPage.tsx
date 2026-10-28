import { Activity, ChevronDown, CircleCheck, CircleStop, CircleX, History, LoaderCircle, Play, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useAction, useResource } from "@/hooks/useResource";
import { formatNumber } from "@/lib/format";
import { format } from "@/lib/route";
import { cn } from "@/lib/utils";
import type { BacnetInfo } from "@/types/generated/BacnetInfo";
import type { Binding } from "@/types/generated/Binding";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { RunInfo } from "@/types/generated/RunInfo";
import type { RunRequest } from "@/types/generated/RunRequest";
import type { RunState } from "@/types/generated/RunState";
import type { SpecSummary } from "@/types/generated/SpecSummary";

import { ErrorNote } from "../ErrorNote";
import { configFacts, describe } from "./describe";
import { FAULT_CHOICES, type FaultKind, faultRequest } from "./faults";
import { ReplayList } from "./ReplayList";
import { RunView } from "./RunView";
import { formatSimTime } from "./trends";

/** Day 185 (days since January 1) is the Monday after the case's peak cooling day; noon is
 * when the plant is busy. */
const DEFAULTS = { day: 185, hour: 12, steps: 300 };
const STEP_S = 120;
const LIST_REFRESH_MS = 10_000;
/** Drafts listed per graph before "show all". */
const SHOWN_DRAFTS = 3;

function StateIcon({ state }: { state: RunState }) {
  switch (state) {
    case "starting":
      return <LoaderCircle aria-hidden="true" />;
    case "running":
      return <Activity aria-hidden="true" />;
    case "completed":
      return <CircleCheck aria-hidden="true" />;
    case "stopped":
      return <CircleStop aria-hidden="true" />;
    case "failed":
      return <CircleX aria-hidden="true" />;
  }
}

const GRAPHS = [
  { spec: "chw_plant_reset", label: "Plant reset graph" },
  { spec: "ahu_chw_requests", label: "Air handler requests graph" },
  { spec: "ahu_static_pressure_reset", label: "Static pressure reset graph" },
  { spec: "zone_pressure_requests", label: "Zone pressure requests graph" },
] as const;

/** Whether a spec's graph runs with these switches (drafts for unused specs are not sent). */
function uses(spec: string, requests: boolean, pressure: boolean): boolean {
  if (spec === "ahu_chw_requests") return requests;
  if (spec === "ahu_static_pressure_reset" || spec === "zone_pressure_requests") return pressure;
  return true;
}

/** Reference graph or one of the spec's verified drafts. */
function GraphChoice({
  label,
  drafts,
  value,
  onChange,
  disabled,
  noneLabel = "Hand-built",
}: {
  label: string;
  drafts: DraftInfo[];
  value: string | null;
  onChange: (contentId: string | null) => void;
  disabled: boolean;
  /** What "no draft" means: our hand-built graph, or (for an engineer's sequence) not running it. */
  noneLabel?: string;
}) {
  // The newest drafts first in view; a chosen older one stays visible.
  const [showAll, setShowAll] = useState(() => value !== null && drafts.findIndex((d) => d.content_id === value) < drafts.length - SHOWN_DRAFTS);
  return (
    <div className="flex flex-col gap-1" role="group" aria-label={label}>
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <div className="flex flex-col gap-1">
        <Button
          size="sm"
          variant={value === null ? "secondary" : "ghost"}
          aria-pressed={value === null}
          disabled={disabled}
          onClick={() => onChange(null)}
          className="justify-start"
        >
          {noneLabel}
        </Button>
        {(showAll ? drafts : drafts.slice(-SHOWN_DRAFTS)).map((d) => (
          <Button
            key={d.content_id}
            size="sm"
            variant={value === d.content_id ? "secondary" : "ghost"}
            aria-pressed={value === d.content_id}
            disabled={disabled}
            onClick={() => onChange(d.content_id)}
            title={d.content_id}
            className="h-auto justify-start py-1.5"
          >
            {d.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
            <span className="flex flex-col items-start text-left">
              <span>
                Drafted …{d.content_id.slice(-6)}
                {d.source === "recorded" ? " (recorded)" : ""}
              </span>
              <span className="text-[11px] font-normal text-muted-foreground">
                {d.model} · {d.source} · {d.attempts_used} attempt{d.attempts_used === 1 ? "" : "s"}
              </span>
            </span>
          </Button>
        ))}
        {drafts.length > SHOWN_DRAFTS && (
          <Button size="sm" variant="link" className="self-start" onClick={() => setShowAll((v) => !v)}>
            {showAll ? "Show fewer" : `Show all ${drafts.length} drafts`}
          </Button>
        )}
      </div>
    </div>
  );
}

/** One click sets the whole form: the demo's usual windows. */
const PRESETS = [
  { id: "demo", label: "Demo window", detail: "Day 185 from noon, 10 h", day: 185, hour: 12, steps: 300, fault: false, baseline: false },
  { id: "short", label: "Short check", detail: "Day 185 from noon, 1 h", day: 185, hour: 12, steps: 30, fault: false, baseline: false },
  { id: "fault", label: "Fault demo", detail: "Day 186 from midnight, 2 h, a test fault at step 20", day: 186, hour: 0, steps: 60, fault: true, baseline: false },
  { id: "baseline", label: "Baseline", detail: "BOPTEST's own controllers over the demo window", day: 185, hour: 12, steps: 300, fault: false, baseline: true },
] as const;

export type DraftChoice = { spec: string; contentId: string };

function StartForm({ onStarted, initialDraft }: { onStarted: (run: RunInfo) => void; initialDraft?: DraftChoice | null }) {
  const [baseline, setBaseline] = useState(false);
  const [requests, setRequests] = useState(true);
  // A draft handed over from the authoring panel turns on what it needs to run.
  const [pressure, setPressure] = useState(
    initialDraft?.spec === "ahu_static_pressure_reset" || initialDraft?.spec === "zone_pressure_requests",
  );
  const [day, setDay] = useState(DEFAULTS.day);
  const [hour, setHour] = useState(DEFAULTS.hour);
  const [steps, setSteps] = useState(DEFAULTS.steps);
  const stored = useResource<DraftInfo[]>("/api/drafts");
  // Engineers' own sequences that are bound to the building, with their titles.
  const bound = useResource<Record<string, Binding>>("/api/workbench/bindings");
  const ownSpecs = useResource<SpecSummary[]>("/api/workbench");
  const own = bound.status === "ready" ? Object.keys(bound.data) : [];
  const bacnet = useResource<BacnetInfo>("/api/bacnet");
  const [fault, setFault] = useState(false);
  const [faultStep, setFaultStep] = useState(30);
  const [faultKind, setFaultKind] = useState<FaultKind>("nan");
  const [chosen, setChosen] = useState<Record<string, string | null>>(
    initialDraft ? { [initialDraft.spec]: initialDraft.contentId } : {},
  );
  const body = useMemo<RunRequest>(() => {
    const drafts: Record<string, string> = {};
    for (const { spec } of GRAPHS) {
      const id = chosen[spec];
      if (id && uses(spec, requests, pressure) && !baseline) drafts[spec] = id;
    }
    for (const spec of own) {
      const id = chosen[spec];
      if (id && !baseline) drafts[spec] = id;
    }
    return {
      baseline,
      requests,
      pressure: pressure && !baseline,
      steps,
      start_s: day * 86400 + hour * 3600,
      ...(Object.keys(drafts).length > 0 ? { drafts } : {}),
      ...(fault && !baseline && requests ? { fault: faultRequest(faultKind, faultStep) } : {}),
    };
  }, [baseline, requests, pressure, steps, day, hour, chosen, fault, faultStep, faultKind, bound]);
  const start = useAction<RunInfo>("/api/runs", body);
  // Each started run is reported exactly once, however often this component re-renders.
  const reported = useRef<string | null>(null);
  useEffect(() => {
    if (start.state.status === "done" && reported.current !== start.state.data.id) {
      reported.current = start.state.data.id;
      onStarted(start.state.data);
    }
  }, [start.state, onStarted]);
  const [more, setMore] = useState(false);
  const applyPreset = (p: (typeof PRESETS)[number]) => {
    setDay(p.day);
    setHour(p.hour);
    setSteps(p.steps);
    setBaseline(p.baseline);
    setFault(p.fault);
    if (p.fault) setFaultStep(20);
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle>Start a run</CardTitle>
        <CardDescription>
          On BOPTEST's large office, with a one-day warm-up. Every sequence is verified again first; one run at a time.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap gap-1" role="group" aria-label="Presets">
          {PRESETS.map((p) => (
            <Button key={p.id} size="sm" variant="outline" title={p.detail} onClick={() => applyPreset(p)}>
              {p.label}
            </Button>
          ))}
        </div>

        <div className="flex flex-col gap-3">
          <span className="text-xs font-medium text-muted-foreground">What runs</span>
          <div className="flex items-center justify-between gap-2">
            <Label htmlFor="run-baseline">Baseline only (write nothing)</Label>
            <Switch id="run-baseline" checked={baseline} onCheckedChange={setBaseline} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <Label htmlFor="run-requests">Air handler requests</Label>
            <Switch id="run-requests" checked={requests && !baseline} disabled={baseline} onCheckedChange={setRequests} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <Label htmlFor="run-pressure">Duct static pressure reset (15 zones, 3 air handlers)</Label>
            <Switch id="run-pressure" checked={pressure && !baseline} disabled={baseline} onCheckedChange={setPressure} />
          </div>
        </div>

        {stored.status === "ready" && stored.data.length > 0 && (
          <div className="flex flex-col gap-3">
            <span className="text-xs font-medium text-muted-foreground">Graphs (verified Muse Spark drafts or our own)</span>
            {GRAPHS.map(({ spec, label }) => {
              const drafts = stored.data.filter((d) => d.spec === spec);
              if (drafts.length === 0) return null;
              return (
                <GraphChoice
                  key={spec}
                  label={label}
                  drafts={drafts}
                  value={chosen[spec] ?? null}
                  onChange={(id) => setChosen((c) => ({ ...c, [spec]: id }))}
                  disabled={baseline || !uses(spec, requests, pressure)}
                />
              );
            })}
          </div>
        )}

        {stored.status === "ready" && own.some((spec) => stored.data.some((d) => d.spec === spec)) && (
          <div className="flex flex-col gap-3">
            <span className="text-xs font-medium text-muted-foreground">
              Your sequences (bound to the building; runs a verified draft on every bound floor)
            </span>
            {own.map((spec) => {
              const drafts = stored.data.filter((d) => d.spec === spec);
              if (drafts.length === 0) return null;
              const title = ownSpecs.status === "ready" ? ownSpecs.data.find((s) => s.name === spec)?.title : undefined;
              return (
                <GraphChoice
                  key={spec}
                  label={title ?? spec}
                  drafts={drafts}
                  value={chosen[spec] ?? null}
                  onChange={(id) => setChosen((c) => ({ ...c, [spec]: id }))}
                  disabled={baseline}
                  noneLabel="Off"
                />
              );
            })}
          </div>
        )}

        <Collapsible open={more} onOpenChange={setMore} className="flex flex-col gap-3">
          <div className="flex items-start justify-between gap-2">
            <p className="text-xs text-muted-foreground">
              {steps} steps of {STEP_S} s = {formatNumber((steps * STEP_S) / 3600, 2)} simulated hours from{" "}
              {formatSimTime(day * 86400 + hour * 3600)}
              {fault && !baseline && requests ? `, test fault from step ${faultStep}` : ""}.
            </p>
            <CollapsibleTrigger asChild>
              <Button size="sm" variant="ghost">
                <ChevronDown aria-hidden="true" className={more ? "rotate-180" : undefined} /> {more ? "Less" : "Change"}
              </Button>
            </CollapsibleTrigger>
          </div>
          <CollapsibleContent className="flex flex-col gap-3">
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
            <div className="flex items-center justify-between gap-2">
              <Label htmlFor="run-fault">Inject a test fault</Label>
              <Switch id="run-fault" checked={fault && !baseline && requests} disabled={baseline || !requests} onCheckedChange={setFault} />
            </div>
            {fault && !baseline && requests && (
              <div className="flex flex-col gap-1">
                <div role="group" aria-label="Kind of test fault" className="flex flex-wrap gap-1">
                  {FAULT_CHOICES.map((c) => (
                    <Button
                      key={c.kind}
                      type="button"
                      size="sm"
                      variant={faultKind === c.kind ? "secondary" : "outline"}
                      aria-pressed={faultKind === c.kind}
                      onClick={() => setFaultKind(c.kind)}
                    >
                      {c.label}
                    </Button>
                  ))}
                </div>
                <Label htmlFor="run-fault-step">From step</Label>
                <Input
                  id="run-fault-step"
                  type="number"
                  min={0}
                  max={Math.max(0, steps - 1)}
                  value={faultStep}
                  onChange={(e) => setFaultStep(Number(e.target.value))}
                />
                <p className="text-xs text-muted-foreground">
                  {FAULT_CHOICES.find((c) => c.kind === faultKind)?.detail}
                </p>
              </div>
            )}
          </CollapsibleContent>
        </Collapsible>

        <Button onClick={start.run} disabled={start.state.status === "running"}>
          <Play aria-hidden="true" /> {start.state.status === "running" ? "Starting…" : "Start"}
        </Button>
        {start.state.status === "error" && <ErrorNote error={start.state.error} />}
        {bacnet.status === "ready" && (
          <p className="text-xs text-muted-foreground">
            Live runs are also mirrored read-only over BACnet/IP: device {bacnet.data.device_instance},{" "}
            {bacnet.data.points.length} objects, UDP port {bacnet.data.port}. Remote writes are refused.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

/** What a listed run ran, in words; a replay says so. */
function runLabel(r: RunInfo): string {
  const what = describe(configFacts(r.config));
  return r.config.replay ? `Replay: ${what}` : what;
}

/** Below the two-column layout the run view sits under the forms: bring it into view. */
function revealOnNarrow(el: HTMLElement | null) {
  if (!el || typeof window.matchMedia !== "function") return;
  if (!window.matchMedia("(min-width: 1024px)").matches) el.scrollIntoView?.({ behavior: "smooth", block: "start" });
}

export function RunsPage({
  initialDraft = null,
  run = null,
  replay = null,
  onSelectRun,
}: {
  /** A verified draft to preset in the start form (from `#/runs?spec=&draft=`). */
  initialDraft?: DraftChoice | null;
  /** The run named in the URL. */
  run?: string | null;
  /** A recorded run to point out (from `#/runs?replay=`); never started by itself. */
  replay?: string | null;
  /** Called with a run just started, so the URL can name it. */
  onSelectRun?: (id: string) => void;
} = {}) {
  const runs = useResource<RunInfo[]>("/api/runs");
  // The page stays mounted under a hidden tab: keep the last run and draft the URL named.
  const [picked, setPicked] = useState<string | null>(run);
  if (run !== null && run !== picked) setPicked(run);
  const [draft, setDraft] = useState<DraftChoice | null>(initialDraft);
  if (initialDraft && (initialDraft.spec !== draft?.spec || initialDraft.contentId !== draft.contentId)) setDraft(initialDraft);
  const list = runs.status === "ready" ? [...runs.data].reverse() : [];
  const current = picked ?? list[0]?.id ?? null;
  const { reload } = runs;
  const view = useRef<HTMLElement>(null);
  const onStarted = useCallback(
    (started: RunInfo) => {
      setPicked(started.id);
      onSelectRun?.(started.id);
      reload();
      revealOnNarrow(view.current);
    },
    [reload, onSelectRun],
  );
  // Keep the list's states and step counts current while any run is active.
  const active = list.some((r) => r.state === "starting" || r.state === "running");
  useEffect(() => {
    if (!active) return;
    const id = setInterval(reload, LIST_REFRESH_MS);
    return () => clearInterval(id);
  }, [active, reload]);
  return (
    <div className="grid gap-4 lg:grid-cols-[20rem_minmax(0,1fr)]">
      <div className="flex flex-col gap-4">
        <StartForm key={draft ? `${draft.spec}:${draft.contentId}` : "none"} onStarted={onStarted} initialDraft={draft} />
        {runs.status === "error" && <ErrorNote error={runs.error} onRetry={reload} />}
        {list.length > 0 && (
          <nav aria-label="Runs" className="flex flex-col gap-1">
            <h3 className="px-3 text-xs font-medium text-muted-foreground">Runs on this server</h3>
            {list.map((r) => (
              <a
                key={r.id}
                href={format({ page: "runs", run: r.id })}
                onClick={() => setPicked(r.id)}
                aria-current={r.id === current ? "page" : undefined}
                className={cn("flex flex-col rounded-md px-3 py-2 text-left text-sm hover:bg-muted focus-visible:outline-2", r.id === current && "bg-muted")}
              >
                <span>{runLabel(r)}</span>
                <span className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">
                    <StateIcon state={r.state} /> {r.state}
                  </Badge>
                  <span className="font-mono">{r.id}</span> · {r.ticks}/{r.config.steps} steps
                </span>
              </a>
            ))}
          </nav>
        )}
        <ReplayList highlight={replay} onStarted={onStarted} />
      </div>
      <section ref={view} className="min-w-0 scroll-mt-20">
        {current ? (
          <RunView runId={current} onEnded={reload} />
        ) : (
          <div className="grid min-h-64 place-items-center rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
            <div className="max-w-md">
              <p className="font-medium text-foreground">No runs on this server yet</p>
              <p className="mt-1">
                Start one on BOPTEST with a preset on the left, or replay a recorded run: it plays through this same view,
                labelled as a replay.
              </p>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
