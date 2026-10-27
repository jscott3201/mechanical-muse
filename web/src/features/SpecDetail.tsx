import { ShieldCheck } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useAction, useResource } from "@/hooks/useResource";
import { formatWithUnit } from "@/lib/format";
import type { GraphView } from "@/types/generated/GraphView";
import type { Receipt as ReceiptDto } from "@/types/generated/Receipt";
import type { SpecDetail as SpecDetailDto } from "@/types/generated/SpecDetail";
import type { TaskPoint } from "@/types/generated/TaskPoint";

import { AuthorPanel } from "./authoring/AuthorPanel";
import { ErrorNote } from "./ErrorNote";
import { GraphCanvas } from "./graph/GraphCanvas";
import { Receipt } from "./Receipt";

function PointTable({
  title,
  second,
  points,
}: {
  title: string;
  second: string;
  points: TaskPoint[];
}) {
  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{title}</TableHead>
            <TableHead>{second}</TableHead>
            <TableHead>Meaning</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {points.map((p) => (
            <TableRow key={p.name}>
              <TableCell className="font-mono text-xs">{p.name}</TableCell>
              <TableCell>
                {p.value !== undefined
                  ? formatWithUnit(p.value, p.unit)
                  : [p.type, p.unit !== "1" && p.unit !== "bool" ? p.unit : null]
                      .filter(Boolean)
                      .join(" · ")}
              </TableCell>
              <TableCell className="whitespace-normal">{p.meaning}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

/** Our hand-built reference graph, as the engine catalog types it. */
function ReferenceGraph({ name }: { name: string }) {
  const graph = useResource<GraphView>(`/api/specs/${encodeURIComponent(name)}/graph`);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Reference block graph</CardTitle>
        <CardDescription>
          Our hand-built graph for this sequence, made only of elementary CDL blocks from the
          engine's catalog. Ports and connection types come from that catalog.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {graph.status === "loading" && <Skeleton className="h-[480px] w-full" />}
        {graph.status === "error" && <ErrorNote error={graph.error} onRetry={graph.reload} />}
        {graph.status === "ready" && <GraphCanvas graph={graph.data} label={`Reference graph of ${name}`} />}
      </CardContent>
    </Card>
  );
}

export function SpecDetail({ name }: { name: string }) {
  const spec = useResource<SpecDetailDto>(`/api/specs/${encodeURIComponent(name)}`);
  const verify = useAction<ReceiptDto>(`/api/specs/${encodeURIComponent(name)}/verify`);
  if (spec.status === "loading") return <Skeleton className="h-64 w-full" />;
  if (spec.status === "error") return <ErrorNote error={spec.error} onRetry={spec.reload} />;
  const { summary, description, params, scenarios } = spec.data;
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{summary.title}</CardTitle>
          <CardDescription className="max-w-prose">{description}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <PointTable title="Input" second="Type" points={summary.inputs} />
          <PointTable title="Output" second="Type" points={summary.outputs} />
          <PointTable title="Parameter" second="Value" points={params} />
        </CardContent>
      </Card>
      <ReferenceGraph name={name} />
      <AuthorPanel spec={name} scenarios={scenarios.length} />
      <Card>
        <CardHeader>
          <CardTitle>Verification</CardTitle>
          <CardDescription>
            Replays all {scenarios.length} scenarios on the pinned engine against our hand-built
            reference graph. Only a passing receipt lets a sequence run on the emulated building (BOPTEST).
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div>
            <Button onClick={verify.run} disabled={verify.state.status === "running"}>
              <ShieldCheck aria-hidden="true" />
              {verify.state.status === "running" ? "Verifying…" : "Verify"}
            </Button>
          </div>
          {verify.state.status === "error" && <ErrorNote error={verify.state.error} />}
          {verify.state.status === "done" && verify.state.data.spec === name && (
            <Receipt receipt={verify.state.data} />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
