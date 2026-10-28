import { History, LoaderCircle, Sparkles, Wrench } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useAction } from "@/hooks/useResource";
import type { FixExplanationView } from "@/types/generated/FixExplanationView";

import { ExplainErrorNote } from "../ErrorNote";

/** Text with `backticked` block ids shown as code; plain text otherwise (never parsed as HTML). */
function WithCode({ text }: { text: string }) {
  const parts = text.split("`");
  // An odd count of backticks leaves the last one unpaired: keep it as text.
  if (parts.length % 2 === 0) return <>{text}</>;
  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <code key={i} className="rounded bg-muted px-1 text-[0.85em]">
            {part}
          </code>
        ) : (
          <span key={i}>{part}</span>
        ),
      )}
    </>
  );
}

/**
 * Muse Spark's account of how attempt `after` repaired the one before it, from the exact
 * difference between the two graphs (computed by the server) and the earlier failures.
 * Model-written and labelled so; the verification result above stays the verdict.
 */
export function FixExplanationPanel({ sessionId, after }: { sessionId: string; after: number }) {
  const explain = useAction<FixExplanationView>(
    `/api/authoring/${encodeURIComponent(sessionId)}/attempts/${after}/explain`,
    {},
  );
  const state = explain.state;
  if (state.status === "idle" || state.status === "running") {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <Button size="sm" variant="outline" onClick={explain.run} disabled={state.status === "running"}>
          {state.status === "running" ? (
            <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" />
          ) : (
            <Wrench aria-hidden="true" />
          )}
          {state.status === "running" ? "Explaining…" : "Explain the fix"}
        </Button>
        <span className="text-xs text-muted-foreground">
          What changed since attempt {after - 1} and why, from the exact difference between the two graphs.
        </span>
      </div>
    );
  }
  if (state.status === "error") return <ExplainErrorNote error={state.error} onRetry={explain.run} />;
  const e = state.data;
  return (
    <section aria-label={`What changed in attempt ${e.after}`} className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">
          {e.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
          Written by {e.model}
          {e.source === "recorded" ? " (recorded)" : ""}
        </Badge>
        <span className="text-xs text-muted-foreground">
          Attempt {e.before} to {e.after}. The verdict above is the engine&apos;s; this is the model&apos;s reading of it.
        </span>
      </div>
      <p className="text-sm">
        <WithCode text={e.summary} />
      </p>
      <ul className="flex flex-col gap-2">
        {e.changes.map((c, i) => (
          <li key={`${i}-${c.title}`}>
            <span className="block text-sm font-medium">{c.title}</span>
            <span className="block text-sm text-muted-foreground">
              <WithCode text={c.text} />
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
