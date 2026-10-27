import { Skeleton } from "@/components/ui/skeleton";
import { useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import type { SpecSummary } from "@/types/generated/SpecSummary";

import { ErrorNote } from "./ErrorNote";

export function SpecList({
  selected,
  onSelect,
}: {
  selected: string | null;
  onSelect: (name: string) => void;
}) {
  const specs = useResource<SpecSummary[]>("/api/specs");
  if (specs.status === "loading") return <Skeleton className="h-24 w-full" />;
  if (specs.status === "error") return <ErrorNote error={specs.error} onRetry={specs.reload} />;
  return (
    <nav aria-label="Sequences" className="flex flex-col gap-1">
      {specs.data.map((s) => (
        <button
          key={s.name}
          type="button"
          onClick={() => onSelect(s.name)}
          aria-current={s.name === selected ? "page" : undefined}
          className={cn(
            "rounded-md px-3 py-2 text-left text-sm hover:bg-muted focus-visible:outline-2",
            s.name === selected && "bg-muted font-medium",
          )}
        >
          <span className="block">{s.title}</span>
          <span className="text-xs text-muted-foreground">
            {s.inputs.length} in · {s.outputs.length} out · {s.scenarios} scenarios
          </span>
        </button>
      ))}
    </nav>
  );
}
