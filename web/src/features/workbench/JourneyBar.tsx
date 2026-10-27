import { Circle, CircleCheck, CircleDot } from "lucide-react";

import { type JourneyFacts, journey, type StepState } from "@/lib/journey";
import { format } from "@/lib/route";
import { cn } from "@/lib/utils";

const STATE_TEXT: Record<StepState, string> = { done: "done", next: "next", todo: "not yet" };

function Icon({ state }: { state: StepState }) {
  if (state === "done") return <CircleCheck aria-hidden="true" className="size-4" />;
  if (state === "next") return <CircleDot aria-hidden="true" className="size-4" />;
  return <Circle aria-hidden="true" className="size-4" />;
}

/** Read → Accept → Draft & verify → Bind → Run, each step marked by icon and words. */
export function JourneyBar(facts: JourneyFacts) {
  const steps = journey(facts);
  return (
    <nav aria-label="Your sequence, step by step">
      <ol className="flex flex-wrap items-center gap-x-1 gap-y-2 text-sm">
        {steps.map((s, i) => {
          const body = (
            <>
              <Icon state={s.state} />
              <span>{s.label}</span>{" "}
              <span className="text-xs text-muted-foreground">({STATE_TEXT[s.state]})</span>
            </>
          );
          const cls = cn(
            "inline-flex items-center gap-1.5 rounded-md px-2 py-1",
            s.state === "next" && "bg-muted font-medium",
            s.state === "todo" && "text-muted-foreground",
          );
          return (
            <li key={s.id} className="flex items-center gap-1">
              {i > 0 && (
                <span aria-hidden="true" className="text-muted-foreground">
                  →
                </span>
              )}
              {s.to ? (
                <a href={format(s.to)} className={cn(cls, "hover:bg-muted focus-visible:outline-2")} aria-current={s.state === "next" ? "step" : undefined}>
                  {body}
                </a>
              ) : (
                <span className={cls}>{body}</span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
