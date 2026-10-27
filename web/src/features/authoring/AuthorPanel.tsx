import { Ban, CircleX, Download, History, LoaderCircle, Play, ShieldCheck, Sparkles, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import type { AttemptEntry } from "@/hooks/authorStream";
import { useAuthorStream } from "@/hooks/useAuthorStream";
import { useAction, useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import { formatNumber } from "@/lib/format";
import type { AttemptView } from "@/types/generated/AttemptView";
import type { Health } from "@/types/generated/Health";
import type { AuthorInfo } from "@/types/generated/AuthorInfo";
import type { AuthorRequest } from "@/types/generated/AuthorRequest";
import type { ModelSource } from "@/types/generated/ModelSource";
import type { Problem } from "@/types/generated/Problem";

import { ErrorNote } from "../ErrorNote";
import { ExplanationPanel } from "../graph/ExplanationPanel";
import { GraphCanvas } from "../graph/GraphCanvas";
import { Outcome } from "../Receipt";

/**
 * Problems grouped by code: the blocks involved, one explanation, and every message behind a
 * toggle. An engine refusal can name every member of a loop, and ten near-identical lines would
 * bury the point.
 */
function ProblemList({ problems }: { problems: Problem[] }) {
  const groups = new Map<string, Problem[]>();
  for (const p of problems) groups.set(p.code, [...(groups.get(p.code) ?? []), p]);
  return (
    <ul className="flex flex-col gap-2 text-sm">
      {[...groups].map(([code, list]) => {
        const nodes = [...new Set(list.map((p) => p.node).filter((n): n is string => Boolean(n)))].sort();
        return (
          <li key={code} className="flex flex-col gap-1">
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <code className="text-xs">{code}</code>
              {nodes.length > 0 && (
                <span className="text-muted-foreground">
                  at{" "}
                  {nodes.map((n, k) => (
                    <span key={n}>
                      {k > 0 && ", "}
                      <span className="font-mono text-xs">{n}</span>
                    </span>
                  ))}
                </span>
              )}
            </p>
            {list.length === 1 ? (
              <p className="text-muted-foreground">{list[0]?.message}</p>
            ) : (
              <details className="text-muted-foreground">
                <summary className="cursor-pointer">
                  {list.length} engine messages
                </summary>
                <ul className="mt-1 flex flex-col gap-1">
                  {list.map((p, k) => (
                    <li key={`${p.subject}-${k}`}>{p.message}</li>
                  ))}
                </ul>
              </details>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** What one checked draft got, with an icon and words (never color alone). */
function Verdict({ view }: { view: AttemptView }) {
  switch (view.result) {
    case "passed":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 font-medium">
            <ShieldCheck aria-hidden="true" className="size-4" /> Passes {view.scenarios} of {view.scenarios} scenarios
          </p>
          <code className="text-xs text-muted-foreground break-all">{view.content_id}</code>
        </div>
      );
    case "replay-failed":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 font-medium">
            <TriangleAlert aria-hidden="true" className="size-4" /> Loads, but passes {view.passed} of {view.total} scenarios
          </p>
          <ul className="flex flex-col gap-1 text-sm">
            {view.failures.map((s) => (
              <li key={s.name}>
                <span className="font-mono text-xs">{s.name}</span>: <Outcome s={s} />
              </li>
            ))}
          </ul>
        </div>
      );
    case "load-failed":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 font-medium">
            <CircleX aria-hidden="true" className="size-4" /> Refused by the engine at {view.stage}:{" "}
            <code className="text-xs">{view.code}</code>
          </p>
          {view.problems.length > 0 ? (
            <ProblemList problems={view.problems} />
          ) : (
            view.detail && <p className="text-sm text-muted-foreground">{view.detail}</p>
          )}
        </div>
      );
    case "invalid":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 font-medium">
            <CircleX aria-hidden="true" className="size-4" /> The validator found {view.problems.length}{" "}
            {view.problems.length === 1 ? "problem" : "problems"}
          </p>
          <ProblemList problems={view.problems} />
        </div>
      );
    case "interface-mismatch":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 font-medium">
            <CircleX aria-hidden="true" className="size-4" /> Inputs or outputs differ from the task
          </p>
          <ul className="text-sm text-muted-foreground">
            {view.problems.map((p) => (
              <li key={p}>{p}</li>
            ))}
          </ul>
        </div>
      );
    case "not-a-graph":
      return (
        <p className="flex items-center gap-2 font-medium">
          <CircleX aria-hidden="true" className="size-4" /> Not a block graph: {view.message}
        </p>
      );
    case "model-error":
    case "harness-error":
      return (
        <p className="flex items-center gap-2 font-medium">
          <Ban aria-hidden="true" className="size-4" />
          {view.result === "model-error" ? "The model call failed" : "Our checker failed"}: {view.message}
        </p>
      );
  }
}

/** The blocks and points a draft's problems name, for outlining in its graph. */
function flaggedNodes(view: AttemptView): string[] {
  const problems = view.result === "invalid" || view.result === "load-failed" ? view.problems : [];
  return [...new Set(problems.map((p) => p.node).filter((n): n is string => Boolean(n)))];
}

function AttemptRow({ attempt, spec }: { attempt: AttemptEntry; spec: string }) {
  const [open, setOpen] = useState(false);
  const [focus, setFocus] = useState<string[]>([]);
  const graph = attempt.view?.graph;
  // Only a verified draft is stored, so only it can be explained by content id.
  const verifiedId = attempt.view?.result === "passed" ? attempt.view.content_id : null;
  return (
    <li className="flex flex-col gap-2 rounded-md border p-3">
      <p className="text-xs text-muted-foreground">
        Attempt {attempt.index} of {attempt.of}
        {attempt.view && attempt.view.usage.output > 0 && ` · ${formatNumber(attempt.view.usage.output, 0)} output tokens`}
      </p>
      {attempt.view ? (
        <>
          <Verdict view={attempt.view} />
          {graph && (
            <div className="flex flex-col gap-2">
              <div>
                <Button size="sm" variant="outline" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
                  {open ? "Hide the drafted graph" : "Show the drafted graph"}
                </Button>
              </div>
              {open && (
                <GraphCanvas
                  graph={graph}
                  flagged={flaggedNodes(attempt.view)}
                  highlighted={focus}
                  label={`Graph drafted in attempt ${attempt.index}`}
                />
              )}
              {open && verifiedId && <ExplanationPanel spec={spec} draft={verifiedId} onFocusBlocks={setFocus} />}
            </div>
          )}
        </>
      ) : (
        <p className="flex items-center gap-2 font-medium">
          <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> Muse Spark is drafting…
        </p>
      )}
    </li>
  );
}

const ENDED_TEXT = {
  verified: "Verified draft stored with its receipt. It can now be chosen for a BOPTEST run.",
  unverified: "No draft passed within the attempt budget. Nothing was stored.",
  failed: "The session stopped on our side; nothing can be concluded about the model.",
  running: "",
} as const;

/** Start an authoring session for `spec` and follow it attempt by attempt. */
export function AuthorPanel({
  spec,
  scenarios,
  onRunDraft,
}: {
  spec: string;
  scenarios: number;
  /** Opens the run form with this verified draft chosen. */
  onRunDraft?: (spec: string, contentId: string) => void;
}) {
  const health = useResource<Health>("/api/health");
  const live = health.status === "ready" && health.data.live_model !== undefined;
  const recordedOk = health.status !== "ready" || health.data.recorded_model;
  const [chosen, setChosen] = useState<ModelSource | null>(null);
  // Default to what the server can do: live when it has a key, else the recorded session.
  const source: ModelSource = chosen ?? (live || health.status !== "ready" ? "live" : "recorded");
  const setSource = setChosen;
  const body = useMemo<AuthorRequest>(() => ({ source }), [source]);
  const start = useAction<AuthorInfo>(`/api/specs/${encodeURIComponent(spec)}/author`, body);
  const [sessionId, setSessionId] = useState<string | null>(null);
  useEffect(() => {
    if (start.state.status === "done") setSessionId(start.state.data.id);
  }, [start.state]);
  const stream = useAuthorStream(sessionId);
  const running = start.state.status === "running" || (sessionId !== null && stream.ended === null);
  const recorded = (stream.config?.source ?? source) === "recorded";
  return (
    <Card>
      <CardHeader>
        <CardTitle>Draft with Muse Spark</CardTitle>
        <CardDescription className="max-w-prose">
          Muse Spark reads this task and writes a block graph. Each draft is checked: interface,
          validator, engine load, then all {scenarios} scenarios. What fails goes back to the model,
          up to {stream.config?.max_attempts ?? 5} attempt{(stream.config?.max_attempts ?? 5) === 1 ? "" : "s"}. A passing draft is stored with its receipt.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="inline-flex rounded-lg bg-muted p-1" role="group" aria-label="Model source">
            <Button
              variant="ghost"
              size="sm"
              aria-pressed={source === "live"}
              onClick={() => setSource("live")}
              disabled={running || !live}
              title={live ? "Call Muse Spark now" : "This server has no model key"}
              className={cn(source === "live" && "bg-background shadow-sm")}
            >
              <Sparkles aria-hidden="true" /> Live model call
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-pressed={source === "recorded"}
              onClick={() => setSource("recorded")}
              disabled={running || !recordedOk}
              title="Play back an earlier live session; every check still runs now"
              className={cn(source === "recorded" && "bg-background shadow-sm")}
            >
              <History aria-hidden="true" /> Recorded run
            </Button>
          </div>
          <Button onClick={start.run} disabled={running || (source === "live" && !live && health.status === "ready")}>
            {running ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : <Sparkles aria-hidden="true" />}
            {running ? "Authoring…" : "Draft"}
          </Button>
          {health.status === "ready" && !live && (
            <span className="text-xs text-muted-foreground">No model key on this server: recorded sessions only.</span>
          )}
        </div>
        {start.state.status === "error" && <ErrorNote error={start.state.error} />}
        {stream.config && (
          <div className="flex flex-wrap items-center gap-2 text-sm">
            {recorded ? (
              <Badge variant="outline">
                <History aria-hidden="true" /> Recorded
              </Badge>
            ) : (
              <Badge variant="outline">
                <Sparkles aria-hidden="true" /> Live
              </Badge>
            )}
            <span>
              {stream.config.model} · effort {stream.config.effort} · repair messages name failing scenarios
            </span>
          </div>
        )}
        {stream.config && recorded && (
          <p className="text-sm text-muted-foreground">
            Recorded run: the model's replies come from an earlier live Muse Spark session and are
            played back. Every check runs now, on this server.
          </p>
        )}
        {stream.attempts.length > 0 && (
          <ol aria-label="Attempts" className="flex flex-col gap-2">
            {stream.attempts.map((a) => (
              <AttemptRow key={a.index} attempt={a} spec={spec} />
            ))}
          </ol>
        )}
        {stream.ended && (
          <section aria-label="Session result" className="flex flex-col gap-1">
            <div className="flex flex-wrap items-center gap-2">
              {stream.ended.state === "verified" ? (
                <Badge>
                  <ShieldCheck aria-hidden="true" /> Verified
                </Badge>
              ) : (
                <Badge variant="destructive">
                  <CircleX aria-hidden="true" /> {stream.ended.state === "failed" ? "Failed" : "Not verified"}
                </Badge>
              )}
              <span className="text-sm">{ENDED_TEXT[stream.ended.state]}</span>
            </div>
            {stream.ended.state === "verified" && stream.ended.draft && (
              <div className="flex flex-wrap gap-2">
                {onRunDraft && (
                  <Button
                    size="sm"
                    onClick={() => {
                      const d = stream.ended?.draft;
                      if (d) onRunDraft(spec, d.content_id);
                    }}
                  >
                    <Play aria-hidden="true" /> Run this draft on BOPTEST
                  </Button>
                )}
                <Button size="sm" variant="outline" asChild>
                  <a href={`/api/drafts/${encodeURIComponent(stream.ended.draft.content_id)}/cxf`} download>
                    <Download aria-hidden="true" /> Download CXF
                  </a>
                </Button>
              </div>
            )}
            {stream.ended.message && <p className="text-sm text-muted-foreground">{stream.ended.message}</p>}
            <p className="text-xs text-muted-foreground">
              {formatNumber(stream.ended.usage.input, 0)} input and {formatNumber(stream.ended.usage.output, 0)} output
              tokens ({formatNumber(stream.ended.usage.reasoning, 0)} reasoning)
              {recorded && ", as recorded"}
            </p>
          </section>
        )}
        {stream.connection === "reconnecting" && (
          <p className="text-sm text-muted-foreground">Connection lost; reconnecting…</p>
        )}
      </CardContent>
    </Card>
  );
}
