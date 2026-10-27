import { ArrowRight, Building2, FileText, Network, ShieldCheck, Sparkles } from "lucide-react";
import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { change } from "@/features/runs/baseline";
import { formatSimTime } from "@/features/runs/trends";
import { useResource } from "@/hooks/useResource";
import { formatNumber } from "@/lib/format";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { Health } from "@/types/generated/Health";
import type { ReplayInfo } from "@/types/generated/ReplayInfo";
import type { SpecSummary } from "@/types/generated/SpecSummary";

export type Destination = "sequences" | "runs";

function Step({ n, icon, title, children, action }: { n: number; icon: ReactNode; title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <Card className="relative">
      <CardHeader>
        <div className="flex items-center gap-2">
          <span className="grid size-7 place-items-center rounded-full bg-accent text-accent-foreground" aria-hidden="true">
            {icon}
          </span>
          <span className="text-xs font-medium text-muted-foreground">Step {n}</span>
        </div>
        <CardTitle className="text-base">{title}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm text-muted-foreground">
        {children}
        {action}
      </CardContent>
    </Card>
  );
}

/** What a recorded run ran, in words. */
function describe(r: ReplayInfo): string {
  const graphs = r.drafted ? "Spark-drafted plant reset" : "Hand-built sequences";
  return r.pressure ? `${graphs}, chilled water and static pressure` : `${graphs}, chilled water`;
}

/** "−24.1 %" style, with the sign spelled out; "no data" when there is nothing to compare. */
function delta(value: number | undefined, base: number | undefined): string {
  const pct = change(value, base);
  if (pct === null) return "no data";
  if (Math.abs(pct) < 0.05) return "no change";
  return `${pct > 0 ? "+" : pct < 0 ? "−" : ""}${formatNumber(Math.abs(pct), 1)} %`;
}

/** Each recorded run set beside the recorded baseline of the same window. */
function Results({ replays }: { replays: ReplayInfo[] }) {
  const rows = replays
    .filter((r) => !r.baseline && r.kpis)
    .map((r) => ({ r, base: replays.find((b) => b.baseline && b.kpis && b.start_s === r.start_s && b.ticks === r.ticks) }))
    .filter((x) => x.base);
  if (rows.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Recorded results on BOPTEST</CardTitle>
        <CardDescription>
          KPIs computed by BOPTEST, each beside BOPTEST's own controllers over the same window. One window per row; see the
          results notes in the repository for what each run does and does not show.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Recorded run</TableHead>
              <TableHead>Window</TableHead>
              <TableHead className="text-right">Energy</TableHead>
              <TableHead className="text-right">Thermal discomfort</TableHead>
              <TableHead className="text-right">CO₂ discomfort</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map(({ r, base }) => (
              <TableRow key={r.name}>
                <TableCell>
                  <span className="flex flex-wrap items-center gap-2">
                    {describe(r)}
                    {r.drafted && (
                      <Badge variant="outline">
                        <Sparkles aria-hidden="true" /> Spark draft
                      </Badge>
                    )}
                  </span>
                  <span className="font-mono text-xs text-muted-foreground">{r.name}</span>
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">
                  {formatSimTime(r.start_s)}, {r.ticks} steps
                </TableCell>
                <TableCell className="text-right tabular-nums">{delta(r.kpis?.ener_tot, base?.kpis?.ener_tot)}</TableCell>
                <TableCell className="text-right tabular-nums">{delta(r.kpis?.tdis_tot, base?.kpis?.tdis_tot)}</TableCell>
                <TableCell className="text-right tabular-nums">{delta(r.kpis?.idis_tot, base?.kpis?.idis_tot)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

/** The pipeline at a glance, with what this server has done at each step. */
export function Overview({ health, onGo }: { health: Health | null; onGo: (to: Destination) => void }) {
  const specs = useResource<SpecSummary[]>("/api/specs");
  const drafts = useResource<DraftInfo[]>("/api/drafts");
  const replays = useResource<ReplayInfo[]>("/api/replays");
  const specCount = specs.status === "ready" ? specs.data.length : null;
  const scenarios = specs.status === "ready" ? specs.data.reduce((n, s) => n + s.scenarios, 0) : null;
  const draftList = drafts.status === "ready" ? drafts.data : [];
  const live = draftList.filter((d) => d.source === "live").length;
  return (
    <div className="flex flex-col gap-6">
      <section className="rounded-xl border bg-card p-6">
        <h2 className="text-xl font-semibold tracking-tight">From a written control sequence to a verified, running controller</h2>
        <p className="mt-2 max-w-3xl text-sm text-muted-foreground">
          Muse Spark reads a sequence written in plain language and drafts it as a graph of standard control blocks. A
          deterministic engine replays test vectors through every draft and sends what fails back to the model. Only a draft
          with a passing receipt may drive the emulated office building, and its live points are mirrored read-only over
          BACnet.
        </p>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button onClick={() => onGo("sequences")}>
            Explore sequences <ArrowRight aria-hidden="true" />
          </Button>
          <Button variant="outline" onClick={() => onGo("runs")}>
            Runs and replays
          </Button>
        </div>
      </section>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-5">
        <Step n={1} icon={<FileText className="size-4" />} title="A sequence in plain words">
          <p>
            {specCount === null ? "Loading…" : `${specCount} sequences`}, each with our own spec text and{" "}
            {scenarios === null ? "" : `${scenarios} `}test scenarios from an independent reference model.
          </p>
        </Step>
        <Step n={2} icon={<Sparkles className="size-4" />} title="Muse Spark drafts it">
          <p>
            Structured output from {health?.live_model ?? "Muse Spark"}; refusals and failing scenarios go back for repair.{" "}
            {draftList.length > 0 && `${draftList.length} verified drafts stored (${live} live, ${draftList.length - live} recorded).`}
          </p>
        </Step>
        <Step n={3} icon={<ShieldCheck className="size-4" />} title="The engine checks it">
          <p>Every draft loads in open-control-engine and replays every scenario. A receipt names its content id and result.</p>
        </Step>
        <Step n={4} icon={<Building2 className="size-4" />} title="It runs the building">
          <p>Verified graphs drive BOPTEST's large office every 120 s; bad data hands control back to BOPTEST's own controllers.</p>
        </Step>
        <Step n={5} icon={<Network className="size-4" />} title="Operators can see it">
          <p>
            {health?.bacnet_device !== undefined && health?.bacnet_device !== null
              ? `Mirrored read-only as BACnet device ${health.bacnet_device}.`
              : "A read-only BACnet/IP mirror of live runs (off on this server)."}
          </p>
        </Step>
      </div>

      {replays.status === "ready" && <Results replays={replays.data} />}

      <p className="text-xs text-muted-foreground">
        Built during the event: the authoring agent, block-graph emitter, engine bridge, runtime, BACnet mirror, web app and
        benchmark harness. Used, not built here: open-control-engine, open-control-library, rusty-bacnet and BOPTEST.
      </p>
    </div>
  );
}
