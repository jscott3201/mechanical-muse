import { ArrowDownRight, ArrowUpRight, ChevronDown, History, Minus, Sparkles } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Progress } from "@/components/ui/progress";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatNumber } from "@/lib/format";
import type { SequenceInfo } from "@/types/generated/SequenceInfo";

import { change, formatChange } from "./baseline";

/** How a sequence's graph was made, in words (the same wording everywhere). */
export function originLabel(s: SequenceInfo): string {
  if (s.origin !== "drafted") return "Hand-built reference";
  return `Drafted by ${s.model ?? "the model"}${s.source === "recorded" ? " (recorded session)" : ""}`;
}

function OriginIcon({ s }: { s: SequenceInfo }) {
  if (s.origin !== "drafted") return null;
  return s.source === "recorded" ? <History aria-hidden="true" className="size-4" /> : <Sparkles aria-hidden="true" className="size-4" />;
}

/** Instances that run the same graph, as one line ("zone… × 15"), with every instance one click away. */
export function SequenceGroups({ sequences }: { sequences: SequenceInfo[] }) {
  const [open, setOpen] = useState(false);
  const groups = new Map<string, SequenceInfo[]>();
  for (const s of sequences) {
    const key = `${s.spec}|${s.content_id}`;
    groups.set(key, [...(groups.get(key) ?? []), s]);
  }
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="flex flex-col gap-2">
      <ul className="grid gap-2 sm:grid-cols-2">
        {[...groups.values()].map((g) => {
          const first = g[0] as SequenceInfo;
          return (
            <li key={`${first.spec}|${first.content_id}`} className="rounded-lg border p-3">
              <p className="flex items-center justify-between gap-2 text-sm font-medium">
                <span className="truncate font-mono text-xs">{first.spec}</span>
                <span className="shrink-0 text-xs text-muted-foreground">× {g.length}</span>
              </p>
              <p className="mt-1 inline-flex items-center gap-1 text-sm">
                <OriginIcon s={first} />
                {originLabel(first)}
              </p>
              <p className="truncate font-mono text-[11px] text-muted-foreground" title={first.content_id}>
                {first.content_id}
              </p>
            </li>
          );
        })}
      </ul>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="self-start">
          <ChevronDown aria-hidden="true" className={open ? "rotate-180" : undefined} />
          {open ? "Hide" : "Show"} all {sequences.length} instances
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Sequence</TableHead>
                <TableHead>Spec</TableHead>
                <TableHead>Verified model (content id)</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sequences.map((s) => (
                <TableRow key={s.name}>
                  <TableCell className="font-mono text-xs">{s.name}</TableCell>
                  <TableCell>{s.spec}</TableCell>
                  <TableCell className="font-mono text-xs break-all">{s.content_id}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/** Steps done out of the run's steps. */
export function RunProgress({ done, total }: { done: number; total: number }) {
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0;
  return (
    <div className="flex items-center gap-3">
      <Progress value={pct} aria-label={`${done} of ${total} steps`} className="h-2" />
      <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
        {done} / {total} steps
      </span>
    </div>
  );
}

const TILES: { key: string; label: string }[] = [
  { key: "ener_tot", label: "Energy" },
  { key: "cost_tot", label: "Operating cost" },
  { key: "tdis_tot", label: "Thermal discomfort" },
  { key: "idis_tot", label: "CO₂ discomfort" },
];

/**
 * The headline KPIs beside another run of the window: change only, the table below has the
 * values. For all four, lower is better. `against` names that run ("baseline", "run without
 * the fault").
 */
export function KpiTiles({
  kpi,
  base,
  against = "baseline",
}: {
  kpi: Record<string, number>;
  base: Record<string, number>;
  against?: string;
}) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4" aria-label={`Change against the ${against === "baseline" ? "baseline run" : against}`}>
      {TILES.map(({ key, label }) => {
        const pct = change(kpi[key], base[key]);
        const flat = pct === null || Math.abs(pct) < 0.05;
        const Icon = flat ? Minus : (pct as number) < 0 ? ArrowDownRight : ArrowUpRight;
        return (
          <Card key={key} className="py-4">
            <CardContent className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">{label} vs {against}</span>
              <span className="flex items-center gap-1 text-2xl font-semibold tabular-nums">
                <Icon aria-hidden="true" className="size-5 text-muted-foreground" />
                {pct === null
                  ? "no data"
                  : flat
                    ? "no change"
                    : formatChange(pct)}
              </span>
              <span className="text-xs text-muted-foreground">{flat ? "Lower is better" : `${(pct as number) < 0 ? "Lower" : "Higher"}; lower is better`}</span>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
