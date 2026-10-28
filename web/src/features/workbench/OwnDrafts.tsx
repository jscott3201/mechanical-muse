import { History, Play, Sparkles } from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useLiveRuns } from "@/hooks/useLiveRuns";
import { useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { GraphView } from "@/types/generated/GraphView";

import { ErrorNote } from "../ErrorNote";
import { ExplanationPanel } from "../graph/ExplanationPanel";
import { GraphCanvas } from "../graph/GraphCanvas";

/** One stored draft's graph, with Spark's explanation beside it. */
function DraftGraph({ spec, draft }: { spec: string; draft: string }) {
  const graph = useResource<GraphView>(`/api/drafts/${encodeURIComponent(draft)}/graph`);
  const [focus, setFocus] = useState<string[]>([]);
  return (
    <div className="grid gap-4 2xl:grid-cols-[minmax(0,1fr)_26rem]">
      <div className="min-w-0">
        {graph.status === "loading" && <Skeleton className="h-[480px] w-full" />}
        {graph.status === "error" && <ErrorNote error={graph.error} onRetry={graph.reload} />}
        {graph.status === "ready" && <GraphCanvas graph={graph.data} highlighted={focus} label={`Verified draft ${draft}`} />}
      </div>
      <div className="2xl:max-h-[640px] 2xl:overflow-auto">
        <ExplanationPanel key={draft} spec={spec} draft={draft} onFocusBlocks={setFocus} />
      </div>
    </div>
  );
}

/**
 * The verified Muse Spark drafts stored for an engineer's own sequence (there is no hand-built
 * graph). One is drawn at a time; a bound sequence can run the chosen one.
 */
export function OwnDrafts({
  spec,
  drafts,
  onDraft,
  onRun,
}: {
  spec: string;
  drafts: DraftInfo[];
  onDraft: () => void;
  /** Present once the sequence is bound to the building. */
  onRun?: (contentId: string) => void;
}) {
  const [chosen, setChosen] = useState<string | null>(null);
  const live = useLiveRuns();
  const shown = drafts.find((d) => d.content_id === chosen) ?? drafts[0];
  if (!shown) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-3 pt-6 text-sm text-muted-foreground">
          <p>
            This is your own sequence, so there is no hand-built graph, and no verified draft is stored yet. Have Muse
            Spark draft one: each verified draft's graph appears here, with its explanation.
          </p>
          <Button variant="outline" onClick={onDraft}>
            <Sparkles aria-hidden="true" /> Draft with Muse Spark
          </Button>
        </CardContent>
      </Card>
    );
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>Verified drafts</CardTitle>
        <CardDescription className="max-w-prose">
          Muse Spark's drafts of your sequence that passed every accepted scenario on the engine, listed by content id.
          There is no hand-built graph to compare with.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <ul className="flex flex-col gap-1" aria-label="Stored drafts">
          {drafts.map((d) => (
            <li key={d.content_id}>
              <button
                type="button"
                onClick={() => setChosen(d.content_id)}
                aria-pressed={d.content_id === shown.content_id}
                className={cn(
                  "flex w-full flex-wrap items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted focus-visible:outline-2",
                  d.content_id === shown.content_id && "bg-muted",
                )}
              >
                <Badge variant="outline">
                  {d.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
                  {d.source === "recorded" ? "recorded" : "live"}
                </Badge>
                <span className="font-mono text-xs">{d.content_id}</span>
                <span className="text-xs text-muted-foreground">
                  {d.model} · {d.attempts_used} attempt{d.attempts_used === 1 ? "" : "s"} · {d.scenarios} scenarios
                </span>
              </button>
            </li>
          ))}
        </ul>
        <div className="flex flex-wrap gap-2">
          {onRun ? (
            <>
              <Button size="sm" onClick={() => onRun(shown.content_id)} disabled={!live.available}>
                <Play aria-hidden="true" /> Run this draft on BOPTEST
              </Button>
              {live.reason && <p className="text-sm text-muted-foreground">{live.reason}</p>}
            </>
          ) : (
            <p className="text-sm text-muted-foreground">To run it on BOPTEST, bind the sequence to the building first (Building tab).</p>
          )}
        </div>
        <DraftGraph key={shown.content_id} spec={spec} draft={shown.content_id} />
      </CardContent>
    </Card>
  );
}
