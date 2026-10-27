import { History, LoaderCircle, MessageSquareText, Sparkles } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useAction } from "@/hooks/useResource";
import type { RunExplanationView } from "@/types/generated/RunExplanationView";

import { ErrorNote } from "../ErrorNote";

/** Muse Spark's account of a run's events in an operator's words. Model-written and labelled so. */
export function RunExplanationPanel({ runId, ended }: { runId: string; ended: boolean }) {
  const explain = useAction<RunExplanationView>(`/api/runs/${encodeURIComponent(runId)}/explain`, {});
  const state = explain.state;
  if (state.status === "idle" || state.status === "running") {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <Button size="sm" variant="outline" onClick={explain.run} disabled={state.status === "running"}>
          {state.status === "running" ? (
            <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" />
          ) : (
            <MessageSquareText aria-hidden="true" />
          )}
          {state.status === "running" ? "Explaining…" : "Explain these events"}
        </Button>
        <span className="text-xs text-muted-foreground">
          Muse Spark reads the events above and says what happened in plain words{ended ? "" : " (so far)"}.
        </span>
      </div>
    );
  }
  if (state.status === "error") return <ErrorNote error={state.error} onRetry={explain.run} />;
  const e = state.data;
  return (
    <section aria-label="What happened" className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">
          {e.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
          Written by {e.model}
          {e.source === "recorded" ? " (recorded)" : ""}
        </Badge>
        <span className="text-xs text-muted-foreground">From the {e.spans} event spans logged; the events themselves are the record.</span>
      </div>
      <p className="text-sm">{e.summary}</p>
      <ol className="flex flex-col gap-2">
        {e.items.map((item, i) => (
          <li key={`${i}-${item.when}`} className="grid gap-x-3 sm:grid-cols-[10rem_minmax(0,1fr)]">
            <span className="text-xs tabular-nums text-muted-foreground">{item.when}</span>
            <span>
              <span className="block text-sm font-medium">{item.title}</span>
              <span className="block text-sm text-muted-foreground">{item.text}</span>
            </span>
          </li>
        ))}
      </ol>
    </section>
  );
}
