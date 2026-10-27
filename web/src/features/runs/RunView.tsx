import { CircleAlert, Info, OctagonX, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { LineChart } from "@/components/viz/LineChart";
import { useAction } from "@/hooks/useResource";
import { useRunStream } from "@/hooks/useRunStream";
import { formatNumber } from "@/lib/format";
import type { RunInfo } from "@/types/generated/RunInfo";

import { ErrorNote } from "../ErrorNote";
import { RunStatus } from "./RunStatus";
import { formatSimTime, trends } from "./trends";

const KPI_LABELS: Record<string, string> = {
  ener_tot: "Energy use (kWh/m²)",
  cost_tot: "Operating cost (per m²)",
  emis_tot: "Emissions (kgCO₂/m²)",
  pele_tot: "Peak electric demand (kW/m²)",
  pgas_tot: "Peak gas demand (kW/m²)",
  tdis_tot: "Thermal discomfort (K·h per zone)",
  idis_tot: "CO₂ discomfort (ppm·h per zone)",
  time_rat: "Computation time ratio",
};

const STALE_MS = 60_000;
const MAX_RESUBSCRIBE = 3;

/** One run: status, the sequences and receipts it loaded, live trends, KPIs and the event log. */
export function RunView({ runId }: { runId: string }) {
  // A gap in the stream (missed events) re-subscribes from the start: the server replays it all.
  const [epoch, setEpoch] = useState(0);
  const onGap = useCallback(() => setEpoch((e) => (e < MAX_RESUBSCRIBE ? e + 1 : e)), []);
  return (
    <>
      {epoch >= MAX_RESUBSCRIBE && (
        <p className="flex items-center gap-2 text-sm text-destructive">
          <CircleAlert aria-hidden="true" className="size-4" /> Events are missing from this stream;
          the charts may have gaps.
        </p>
      )}
      <RunStream key={`${runId}:${epoch}`} runId={runId} onGap={onGap} />
    </>
  );
}

function RunStream({ runId, onGap }: { runId: string; onGap: () => void }) {
  const stream = useRunStream(runId);
  const stop = useAction<RunInfo>(`/api/runs/${encodeURIComponent(runId)}/stop`, {});
  const [lastEventAt, setLastEventAt] = useState(() => Date.now());
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => setLastEventAt(Date.now()), [stream.lastSeq]);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 5000);
    return () => clearInterval(id);
  }, []);
  useEffect(() => {
    if (stream.gap) onGap();
  }, [stream.gap, onGap]);
  const t = useMemo(() => trends(stream.ticks), [stream.ticks]);
  const stale = stream.phase === "running" && !stream.ended && now - lastEventAt > STALE_MS;
  const cfg = stream.config;
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-sm">{runId}</span>
            <RunStatus stream={stream} connection={stream.connection} stale={stale} />
          </CardTitle>
          <CardDescription>
            {cfg
              ? `${cfg.baseline ? "Baseline: BOPTEST's own controllers, nothing written." : cfg.requests ? "Air handler requests feeding the plant reset." : "Plant reset with the request count fixed at 0."} From ${formatSimTime(cfg.start_s)}, ${cfg.steps} steps of ${cfg.step_s} s, ${formatNumber(cfg.warmup_s / 3600, 1)} h warm-up, on BOPTEST ${cfg.case}.`
              : "Waiting for the run to start…"}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {stream.ended?.message && (
            <p className="flex items-start gap-2 text-sm text-destructive">
              <CircleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
              {stream.ended.message}
            </p>
          )}
          {stream.sequences.length > 0 && (
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
                  {stream.sequences.map((s) => (
                    <TableRow key={s.name}>
                      <TableCell className="font-mono text-xs">{s.name}</TableCell>
                      <TableCell>{s.spec}</TableCell>
                      <TableCell className="font-mono text-xs break-all">{s.content_id}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
          {!stream.ended && stream.phase !== null && (
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="outline" size="sm" onClick={stop.run} disabled={stop.state.status === "running"}>
                <OctagonX aria-hidden="true" /> Stop run
              </Button>
              {stream.phase !== "running" && (
                <span className="text-xs text-muted-foreground">
                  A stop during BOPTEST's warm-up takes effect once it finishes.
                </span>
              )}
            </div>
          )}
          {stop.state.status === "error" && <ErrorNote error={stop.state.error} />}
        </CardContent>
      </Card>

      {stream.ticks.length > 0 && (
        <div className="grid gap-4 lg:grid-cols-2">
          <LineChart title="Chilled-water supply temperature" unit="°C" series={t.supplyTemperature} formatX={formatSimTime} formatY={(v) => formatNumber(v, 2)} />
          <LineChart title="Loop differential pressure" unit="kPa" series={t.pressure} formatX={formatSimTime} formatY={(v) => formatNumber(v, 1)} />
          <LineChart title="Reset requests, all air handlers" unit="count" series={t.requests} step formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
          <LineChart title="Cooling coil valves" unit="% open" series={t.valves} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
          <LineChart title="Chiller power" unit="kW" series={t.chillerPower} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
        </div>
      )}

      {stream.kpi && (
        <Card>
          <CardHeader>
            <CardTitle>KPIs</CardTitle>
            <CardDescription>Computed by BOPTEST for this run. Compare with a baseline run of the same window.</CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableBody>
                {Object.entries(stream.kpi).map(([k, v]) => (
                  <TableRow key={k}>
                    <TableCell>{KPI_LABELS[k] ?? k}</TableCell>
                    <TableCell className="text-right tabular-nums">{formatNumber(v, 4)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Events</CardTitle>
          <CardDescription>Phases, the start, fail-safe actions and the end. Never trimmed.</CardDescription>
        </CardHeader>
        <CardContent>
          <ol className="flex max-h-72 flex-col gap-1 overflow-auto text-sm">
            {[...stream.log].reverse().map((e, i) => (
              <li key={`${e.seq}-${i}`} className="flex items-start gap-2">
                {e.severity === "error" ? (
                  <CircleAlert aria-label="error" className="mt-0.5 size-4 shrink-0 text-destructive" />
                ) : e.severity === "warning" ? (
                  <TriangleAlert aria-label="warning" className="mt-0.5 size-4 shrink-0" />
                ) : (
                  <Info aria-label="info" className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="tabular-nums text-muted-foreground">{e.t === null ? "" : formatSimTime(e.t)}</span>
                <span>{e.text}</span>
              </li>
            ))}
          </ol>
        </CardContent>
      </Card>
    </div>
  );
}
