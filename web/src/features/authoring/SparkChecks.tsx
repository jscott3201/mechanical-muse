import { CircleCheck, CircleX, Wrench } from "lucide-react";

import type { ToolCallView } from "@/types/generated/ToolCallView";

import { checkText } from "./checkText";

/**
 * Spark's own checks while it drafted one attempt, labelled as such: they help the model, and
 * never count as verification. The attempt's verdict below is the receipt.
 */
export function SparkChecks({ checks }: { checks: ToolCallView[] }) {
  if (checks.length === 0) return null;
  return (
    <details className="rounded-md border border-dashed p-2 text-sm" open>
      <summary className="flex cursor-pointer items-center gap-2 text-xs font-medium text-muted-foreground">
        <Wrench aria-hidden="true" className="size-3.5" />
        Spark's own checks while drafting ({checks.length}): its tool calls, not the receipt
      </summary>
      <ol className="mt-2 flex flex-col gap-1" aria-label="Spark's own checks">
        {checks.map((c, k) => (
          <li key={`${k}-${c.name}`} className="flex items-start gap-2">
            {c.ok ? (
              <CircleCheck aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
            ) : (
              <CircleX aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
            )}
            <span>
              <span className="sr-only">{c.ok ? "OK: " : "Problem: "}</span>
              {checkText(c)}
            </span>
          </li>
        ))}
      </ol>
    </details>
  );
}
