import { CircleAlert, History, Info, OctagonX, Sparkles, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { LineChart } from "@/components/viz/LineChart";
import { useBaselineKpis } from "@/hooks/useBaselineKpis";
import { useAction } from "@/hooks/useResource";
import { useRunStream } from "@/hooks/useRunStream";
import { formatNumber } from "@/lib/format";
import type { RunInfo } from "@/types/generated/RunInfo";

import { ErrorNote } from "../ErrorNote";
import { change } from "./baseline";
import { RunStatus } from "./RunStatus";
import { KpiTiles, RunProgress, SequenceGroups } from "./RunSummary";
import { formatSimTime, trends } from "./trends";

const KPI_LABELS: Record<string, string> = {
  ener_tot: "Energy use (kWh/m²)",
  cost_tot: "Operating cost (per m²)",
  emis_tot: "Emissions (kgCO₂/m²)",
  pele_tot: "Peak electric demand (kW/m²)",
  pgas_tot: "Peak gas demand (kW/m²)",
  tdis_tot: "Thermal discomfort (K·h per zone)",
  idis_tot: "CO₂ discomfort (ppm·h per zone)",
  pdih_tot: "Peak district heating demand (kW/m²)",
  time_rat: "Computation time ratio",
};

const STALE_MS = 60_000;
const MAX_RESUBSCRIBE = 3;
/** Three air handlers send up to 3 requests each; 0 to 3 keeps a quiet afternoon readable. */
const REQUESTS_DOMAIN: [number, number] = [0, 3];
const PERCENT_DOMAIN: [number, number] = [0, 100];

/** One run: status, the sequences and receipts it loaded, live trends, KPIs and the event log. */
export function RunView({ runId, onEnded }: { runId: string; onEnded?: () => void }) {
  // A gap in the stream (missed events) re-subscribes from the start: the server replays it all.
  const [epoch, setEpoch] = useState(0);
  const [stillGapped, setStillGapped] = useState(false);
  const onGap = useCallback(
    () =>
      setEpoch((e) => {
        if (e >= MAX_RESUBSCRIBE) setStillGapped(true);
        return e < MAX_RESUBSCRIBE ? e + 1 : e;
      }),
    [],
  );
  return (
    <>
      {stillGapped && (
        <p className="flex items-center gap-2 text-sm text-destructive">
          <CircleAlert aria-hidden="true" className="size-4" /> Events are missing from this stream;
          the charts may have gaps.
        </p>
      )}
      <RunStream key={`${runId}:${epoch}`} runId={runId} onGap={onGap} onEnded={onEnded} />
    </>
  );
}

function RunStream({
  runId,
  onGap,
  onEnded,
}: {
  runId: string;
  onGap: () => void;
  onEnded?: (() => void) | undefined;
}) {
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
  const ended = stream.ended !== null;
  useEffect(() => {
    if (ended) onEnded?.();
  }, [ended, onEnded]);
  const t = useMemo(() => trends(stream.ticks), [stream.ticks]);
  const age = now - lastEventAt;
  const staleS = stream.phase === "running" && !stream.ended && age > STALE_MS ? Math.round(age / 1000) : null;
  const cfg = stream.config;
  const base = useBaselineKpis(cfg ?? null, stream.kpi !== null && stream.kpi !== undefined);
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-sm">{runId}</span>
            <RunStatus stream={stream} connection={stream.connection} staleS={staleS} />
            {cfg?.replay && (
              <Badge variant="outline">
                <History aria-hidden="true" /> Replay of a recorded run
              </Badge>
            )}
          </CardTitle>
          <CardDescription>
            {cfg?.replay && (
              <span className="block">
                Recorded events from {cfg.replay.recorded_run}, sent again at a fixed pace. Nothing runs
                against BOPTEST now.
              </span>
            )}
            {cfg
              ? `${cfg.baseline ? "Baseline: BOPTEST's own controllers, nothing written." : cfg.requests ? "Air handler requests feeding the plant reset." : "Plant reset with the request count fixed at 0."}${cfg.pressure ? " Duct static pressure reset on every floor, from its zones' requests." : ""} From ${formatSimTime(cfg.start_s)}, ${cfg.steps} steps of ${cfg.step_s} s, ${formatNumber(cfg.warmup_s / 3600, 1)} h warm-up, on BOPTEST ${cfg.case}.`
              : "Waiting for the run to start…"}
            {cfg?.fault && (
              <span className="block">
                Test fault: {cfg.fault.point} reads NaN for steps {cfg.fault.from_step}–
                {cfg.fault.from_step + cfg.fault.steps - 1}.
              </span>
            )}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {stream.ended?.message && (
            <p className="flex items-start gap-2 text-sm text-destructive">
              <CircleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
              {stream.ended.message}
            </p>
          )}
          {cfg && stream.phase === "running" && !stream.ended && (
            <RunProgress done={stream.ticks.length} total={cfg.steps} />
          )}
          {stream.sequences.length > 0 && <SequenceGroups sequences={stream.sequences} />}
          {!stream.ended && stream.phase !== null && (
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="outline" size="sm" onClick={stop.run} disabled={stop.state.status === "running"}>
                <OctagonX aria-hidden="true" /> Stop run
              </Button>
              {stop.state.status === "done" && (
                <span className="text-xs text-muted-foreground">Stop requested.</span>
              )}
              {stream.phase !== "running" && !cfg?.replay && (
                <span className="text-xs text-muted-foreground">
                  A stop during BOPTEST's warm-up takes effect once it finishes.
                </span>
              )}
            </div>
          )}
          {stop.state.status === "error" && <ErrorNote error={stop.state.error} />}
        </CardContent>
      </Card>

      {stream.kpi && base && <KpiTiles kpi={stream.kpi} base={base.kpis} />}

      {stream.ticks.length > 0 && (
        <div className="grid gap-4 lg:grid-cols-2">
          <LineChart title="Chilled-water supply temperature" unit="°C" series={t.supplyTemperature} formatX={formatSimTime} formatY={(v) => formatNumber(v, 2)} />
          <LineChart title="Loop differential pressure" unit="kPa" series={t.pressure} formatX={formatSimTime} formatY={(v) => formatNumber(v, 1)} />
          <LineChart title="Reset requests, all air handlers" unit="count" series={t.requests} step domain={REQUESTS_DOMAIN} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
          <LineChart title="Cooling coil valves" unit="% open" series={t.valves} domain={PERCENT_DOMAIN} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
          <LineChart title="Chiller power" unit="kW" series={t.chillerPower} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
          {cfg?.pressure && (
            <>
              <LineChart title="Duct static pressure setpoint sent" unit="Pa" series={t.ductSetpoints} formatX={formatSimTime} formatY={(v) => formatNumber(v, 0)} />
              <LineChart title="Supply fan power" unit="kW" series={t.fanPower} formatX={formatSimTime} formatY={(v) => formatNumber(v, 1)} />
            </>
          )}
        </div>
      )}

      {stream.kpi && (
        <Card>
          <CardHeader>
            <CardTitle>KPIs</CardTitle>
            <CardDescription>
              {cfg?.replay
                ? `Recorded with run ${cfg.replay.recorded_run}, as BOPTEST computed them then; not recomputed by this replay.`
                : "Computed by BOPTEST for this run."}{" "}
              {base
                ? `Beside the ${base.source}: BOPTEST's own controllers over the same window, nothing written.`
                : cfg?.baseline
                  ? "This is a baseline run: BOPTEST's own controllers, nothing written."
                  : "No baseline run of the same window yet; run one (Baseline only) to compare."}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              {base && (
                <TableHeader>
                  <TableRow>
                    <TableHead>KPI</TableHead>
                    <TableHead className="text-right">Baseline</TableHead>
                    <TableHead className="text-right">This run</TableHead>
                    <TableHead className="text-right">Change</TableHead>
                  </TableRow>
                </TableHeader>
              )}
              <TableBody>
                {Object.entries(stream.kpi).map(([k, v]) => {
                  const b = base?.kpis[k];
                  const pct = change(v, b);
                  return (
                    <TableRow key={k}>
                      <TableCell>{KPI_LABELS[k] ?? k}</TableCell>
                      {base && (
                        <TableCell className="text-right tabular-nums">{b === undefined ? "no data" : formatNumber(b, 4)}</TableCell>
                      )}
                      <TableCell className="text-right tabular-nums">{formatNumber(v, 4)}</TableCell>
                      {base && (
                        <TableCell className="text-right tabular-nums">
                          {pct === null ? "—" : `${pct > 0 ? "+" : pct < 0 ? "−" : ""}${formatNumber(Math.abs(pct), 1)} %`}
                        </TableCell>
                      )}
                    </TableRow>
                  );
                })}
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
