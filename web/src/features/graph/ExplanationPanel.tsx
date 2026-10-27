import { History, LoaderCircle, MessageSquareText, Sparkles } from "lucide-react";
import { useMemo } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useAction } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import type { ExplainRequest } from "@/types/generated/ExplainRequest";
import type { ExplanationView } from "@/types/generated/ExplanationView";

import { ErrorNote } from "../ErrorNote";

/**
 * Muse Spark's plain-language reading of a graph. Model-written and labelled so; it explains, it
 * never controls. Hovering or focusing a part outlines its blocks (via `onFocusBlocks`).
 */
export function ExplanationPanel({
  spec,
  draft,
  onFocusBlocks,
}: {
  spec: string;
  /** A verified draft's content id; absent for our reference graph. */
  draft?: string;
  onFocusBlocks: (blocks: string[]) => void;
}) {
  const body = useMemo<ExplainRequest>(() => (draft ? { graph: draft } : {}), [draft]);
  const explain = useAction<ExplanationView>(`/api/specs/${encodeURIComponent(spec)}/explain`, body);
  const state = explain.state;
  if (state.status === "idle" || state.status === "running") {
    return (
      <div className="flex flex-col gap-2 rounded-lg border border-dashed p-4 text-sm text-muted-foreground">
        <p>Ask Muse Spark to explain this graph in plain words, part by part. Hover a part to see its blocks.</p>
        <Button variant="outline" className="self-start" onClick={explain.run} disabled={state.status === "running"}>
          {state.status === "running" ? (
            <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" />
          ) : (
            <MessageSquareText aria-hidden="true" />
          )}
          {state.status === "running" ? "Explaining…" : "Explain with Muse Spark"}
        </Button>
      </div>
    );
  }
  if (state.status === "error") return <ErrorNote error={state.error} onRetry={explain.run} />;
  const e = state.data;
  return (
    <section aria-label="Explanation" className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">
          {e.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
          Written by {e.model}
          {e.source === "recorded" ? " (recorded)" : ""}
        </Badge>
        <span className="text-xs text-muted-foreground">An explanation for people; the receipt, not this text, decides what runs.</span>
      </div>
      <p className="text-sm">{e.summary}</p>
      <ol className="flex flex-col gap-1" onMouseLeave={() => onFocusBlocks([])}>
        {e.parts.map((p, i) => (
          <li key={`${i}-${p.title}`}>
            <button
              type="button"
              className={cn("w-full rounded-md p-2 text-left hover:bg-muted focus-visible:bg-muted focus-visible:outline-2")}
              onMouseEnter={() => onFocusBlocks(p.blocks)}
              onFocus={() => onFocusBlocks(p.blocks)}
              onBlur={() => onFocusBlocks([])}
            >
              <span className="block text-sm font-medium">
                {i + 1}. {p.title}
              </span>
              <span className="block text-sm text-muted-foreground">{p.text}</span>
              <span className="mt-1 block font-mono text-[11px] text-muted-foreground">{p.blocks.join(", ")}</span>
            </button>
          </li>
        ))}
      </ol>
      {e.unknown_blocks.length > 0 && (
        <p className="text-xs text-muted-foreground">
          The model also named {e.unknown_blocks.length} block{e.unknown_blocks.length === 1 ? "" : "s"} this graph does not have (
          {e.unknown_blocks.join(", ")}); they were left out.
        </p>
      )}
    </section>
  );
}
