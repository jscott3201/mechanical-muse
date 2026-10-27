import { FileText, ListChecks, ShieldCheck, Sparkles, Waypoints } from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
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

/** The spec's opening paragraph, without its markdown code marks. */
function plain(text: string): string {
  return text.replace(/`/g, "");
}

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;

type Section = "interface" | "graph" | "spark" | "scenarios" | "task";

export function SpecDetail({ name }: { name: string }) {
  const spec = useResource<SpecDetailDto>(`/api/specs/${encodeURIComponent(name)}`);
  const verify = useAction<ReceiptDto>(`/api/specs/${encodeURIComponent(name)}/verify`);
  const [section, setSection] = useState<Section>("interface");
  if (spec.status === "loading") return <Skeleton className="h-64 w-full" />;
  if (spec.status === "error") return <ErrorNote error={spec.error} onRetry={spec.reload} />;
  const { summary, about, description, params, scenarios } = spec.data;
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0 flex-1">
              <CardTitle className="text-lg">{summary.title}</CardTitle>
              <CardDescription className="mt-1 max-w-prose">{plain(about) || description}</CardDescription>
              <div className="mt-3 flex flex-wrap gap-2">
                <Badge variant="secondary">{count(summary.inputs.length, "input")}</Badge>
                <Badge variant="secondary">{count(summary.outputs.length, "output")}</Badge>
                <Badge variant="secondary">{count(params.length, "parameter")}</Badge>
                <Badge variant="secondary">{count(scenarios.length, "test scenario")}</Badge>
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button variant="outline" onClick={() => setSection("spark")}>
                <Sparkles aria-hidden="true" /> Draft with Muse Spark
              </Button>
              <Button onClick={verify.run} disabled={verify.state.status === "running"}>
                <ShieldCheck aria-hidden="true" />
                {verify.state.status === "running" ? "Verifying…" : "Verify"}
              </Button>
            </div>
          </div>
        </CardHeader>
        {(verify.state.status === "error" || (verify.state.status === "done" && verify.state.data.spec === name)) && (
          <CardContent>
            {verify.state.status === "error" && <ErrorNote error={verify.state.error} />}
            {verify.state.status === "done" && verify.state.data.spec === name && <Receipt receipt={verify.state.data} />}
          </CardContent>
        )}
      </Card>

      <Tabs value={section} onValueChange={(v) => setSection(v as Section)} className="gap-4">
        <TabsList className="flex-wrap">
          <TabsTrigger value="interface">Interface</TabsTrigger>
          <TabsTrigger value="graph">
            <Waypoints aria-hidden="true" /> Block graph
          </TabsTrigger>
          <TabsTrigger value="spark">
            <Sparkles aria-hidden="true" /> Muse Spark
          </TabsTrigger>
          <TabsTrigger value="scenarios">
            <ListChecks aria-hidden="true" /> Test scenarios
          </TabsTrigger>
          <TabsTrigger value="task">
            <FileText aria-hidden="true" /> Task text
          </TabsTrigger>
        </TabsList>
        <TabsContent value="interface">
          <Card>
            <CardContent className="grid gap-6 pt-6 xl:grid-cols-2">
              <PointTable title="Input" second="Type" points={summary.inputs} />
              <PointTable title="Output" second="Type" points={summary.outputs} />
              <div className="xl:col-span-2">
                <PointTable title="Parameter" second="Value" points={params} />
              </div>
            </CardContent>
          </Card>
        </TabsContent>
        <TabsContent value="graph">
          <ReferenceGraph name={name} />
        </TabsContent>
        <TabsContent value="spark">
          <AuthorPanel spec={name} scenarios={scenarios.length} />
        </TabsContent>
        <TabsContent value="scenarios">
          <Card>
            <CardHeader>
              <CardTitle>Test scenarios</CardTitle>
              <CardDescription>
                Written before any model saw this sequence, from an independent reference model. Every draft, and our own
                graph, must pass all {scenarios.length} on the pinned engine; Verify replays them against our graph.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <ol className="flex flex-col divide-y">
                {scenarios.map((sc, i) => (
                  <li key={sc.name} className="flex gap-3 py-3">
                    <span className="w-6 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{i + 1}</span>
                    <div className="min-w-0">
                      <p className="font-mono text-xs">{sc.name}</p>
                      <p className="text-sm text-muted-foreground">{sc.description}</p>
                    </div>
                  </li>
                ))}
              </ol>
            </CardContent>
          </Card>
        </TabsContent>
        <TabsContent value="task">
          <Card>
            <CardHeader>
              <CardTitle>Task text</CardTitle>
              <CardDescription>Exactly what Muse Spark is given for this sequence, besides the interface and parameters.</CardDescription>
            </CardHeader>
            <CardContent>
              <p className="max-w-prose text-sm leading-relaxed whitespace-pre-line">{description}</p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
