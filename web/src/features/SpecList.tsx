import { Skeleton } from "@/components/ui/skeleton";
import { useResource } from "@/hooks/useResource";
import { format } from "@/lib/route";
import { cn } from "@/lib/utils";
import type { SpecSummary } from "@/types/generated/SpecSummary";

import { ErrorNote } from "./ErrorNote";

function Entry({ s, selected }: { s: SpecSummary; selected: boolean }) {
  return (
    <a
      href={format({ page: "sequences", spec: s.name })}
      aria-current={selected ? "page" : undefined}
      className={cn(
        "rounded-md px-3 py-2 text-left text-sm hover:bg-muted focus-visible:outline-2",
        selected && "bg-muted font-medium",
      )}
    >
      <span className="block">{s.title}</span>
      <span className="text-xs text-muted-foreground">
        {s.inputs.length} in · {s.outputs.length} out · {s.scenarios} scenarios
      </span>
    </a>
  );
}

/** Our sequences, then the engineers' own; each entry links to its page. */
export function SpecList({ selected }: { selected: string | null }) {
  const specs = useResource<SpecSummary[]>("/api/specs");
  const own = useResource<SpecSummary[]>("/api/workbench");
  if (specs.status === "loading") return <Skeleton className="h-24 w-full" />;
  if (specs.status === "error") return <ErrorNote error={specs.error} onRetry={specs.reload} />;
  const yours = own.status === "ready" ? own.data : [];
  return (
    <nav aria-label="Sequences" className="flex flex-col gap-1">
      {specs.data.map((s) => (
        <Entry key={s.name} s={s} selected={s.name === selected} />
      ))}
      {yours.length > 0 && (
        <>
          <p className="mt-3 px-3 text-xs font-medium text-muted-foreground">Your sequences</p>
          {yours.map((s) => (
            <Entry key={s.name} s={s} selected={s.name === selected} />
          ))}
        </>
      )}
    </nav>
  );
}
