import { Factory, FlaskConical, TriangleAlert, Zap } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { Comparisons } from "@/hooks/useComparisons";
import { useLedgerComparison, useRunLedger } from "@/hooks/useLedger";
import { formatNumber } from "@/lib/format";
import type { Amounts } from "@/types/generated/Amounts";
import type { Fault } from "@/types/generated/Fault";
import type { LedgerComparison } from "@/types/generated/LedgerComparison";
import type { RunLedger } from "@/types/generated/RunLedger";

import { LEDGER_FOOTER, faultBanner, percentOf, signed, signedUsd } from "./ledger";

/** The run's ledger and its comparisons, fetched once the run has reported BOPTEST's KPIs. */
export function RunEmissions({
  runId,
  hasKpis,
  fault,
  comparisons,
}: {
  runId: string;
  hasKpis: boolean;
  fault: Fault | undefined;
  comparisons: Comparisons;
}) {
  const ledger = useRunLedger(runId, hasKpis);
  const self = hasKpis ? `run:${runId}` : null;
  const vsBase = useLedgerComparison(comparisons.baseline?.reference ?? null, self);
  const vsHealthy = useLedgerComparison(comparisons.healthy?.reference ?? null, self);
  if (!hasKpis || ledger.status !== "ready") return null;
  // A refused comparison (another window after all) just leaves the comparison out.
  const ready = (r: typeof vsBase) => (r?.status === "ready" ? r.data : null);
  return (
    <EmissionsPanel
      ledger={ledger.data}
      vsBaseline={ready(vsBase)}
      baselineSource={comparisons.baseline?.source ?? null}
      fault={fault}
      vsHealthy={ready(vsHealthy)}
      healthySource={comparisons.healthy?.source ?? null}
    />
  );
}

function Change({ d, base, unit }: { d: number; base: number; unit: string }) {
  const pct = percentOf(d, base);
  return (
    <span className="text-xs text-muted-foreground tabular-nums">
      vs baseline: {signed(d)} {unit}
      {pct === null ? "" : ` (${signed(pct, 1)} %)`}
    </span>
  );
}

function ScopeCard({
  title,
  note,
  icon,
  a,
  d,
  base,
}: {
  title: string;
  note: string;
  icon: React.ReactNode;
  a: Amounts;
  d: Amounts | null;
  base: Amounts | null;
}) {
  return (
    <Card className="py-4">
      <CardContent className="flex flex-col gap-1">
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          {icon}
          {title}
        </span>
        <span className="text-2xl font-semibold tabular-nums">{formatNumber(a.kg_co2e, 0)} kgCO₂e</span>
        <span className="text-sm tabular-nums">
          {formatNumber(a.kwh, 0)} kWh · ${formatNumber(a.usd, 0)}
        </span>
        {d && base && <Change d={d.kg_co2e} base={base.kg_co2e} unit="kg" />}
        <span className="text-xs text-muted-foreground">{note}</span>
      </CardContent>
    </Card>
  );
}

/** Scope 1 and 2 cards, end uses, the change against the baseline, comfort beside it, and the fault's cost. */
export function EmissionsPanel({
  ledger,
  vsBaseline,
  baselineSource,
  fault,
  vsHealthy,
  healthySource,
}: {
  ledger: RunLedger;
  vsBaseline: LedgerComparison | null;
  baselineSource: string | null;
  fault: Fault | undefined;
  vsHealthy: LedgerComparison | null;
  healthySource: string | null;
}) {
  const d = vsBaseline?.delta ?? null;
  const base = vsBaseline?.a ?? null;
  const deltaById = new Map((d?.end_uses ?? []).map((e) => [e.id, e.amounts]));
  return (
    <Card>
      <CardHeader>
        <CardTitle>Energy &amp; emissions</CardTitle>
        <CardDescription>
          Scope 1 (gas burnt on site) and Scope 2 (electricity bought), split exactly from BOPTEST&apos;s energy and
          emission KPIs for this run.
          {vsBaseline && baselineSource ? ` Changes are against the ${baselineSource}, same window.` : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {fault && (
          <Alert>
            <FlaskConical aria-hidden="true" />
            <AlertTitle>Injected test fault</AlertTitle>
            <AlertDescription>
              {faultBanner(
                fault,
                vsHealthy && healthySource ? { delta: vsHealthy.delta.total, source: healthySource } : null,
              )}
            </AlertDescription>
          </Alert>
        )}
        {!ledger.reconciles && (
          <p className="flex items-center gap-2 text-sm text-destructive">
            <TriangleAlert aria-hidden="true" className="size-4" /> The split does not re-price to BOPTEST&apos;s
            cost (${formatNumber(ledger.boptest_cost_usd, 2)}); treat these numbers with care.
          </p>
        )}
        <div className="grid gap-3 sm:grid-cols-3">
          <ScopeCard
            title="Scope 1"
            note="Gas: the boiler (zone reheat)"
            icon={<Factory aria-hidden="true" className="size-4" />}
            a={ledger.scope1}
            d={d?.scope1 ?? null}
            base={base?.scope1 ?? null}
          />
          <ScopeCard
            title="Scope 2"
            note="Electricity: chillers, pumps, towers, fans"
            icon={<Zap aria-hidden="true" className="size-4" />}
            a={ledger.scope2}
            d={d?.scope2 ?? null}
            base={base?.scope2 ?? null}
          />
          <Card className="py-4">
            <CardContent className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Total and comfort</span>
              <span className="text-2xl font-semibold tabular-nums">${formatNumber(ledger.total.usd, 0)}</span>
              {d && <span className="text-xs text-muted-foreground tabular-nums">vs baseline: {signedUsd(d.total.usd)}</span>}
              <span className="text-sm tabular-nums">
                Thermal discomfort {ledger.tdis_tot === undefined ? "no data" : `${formatNumber(ledger.tdis_tot, 3)} K·h per zone`}
              </span>
              {d?.tdis_tot !== undefined && d !== null && (
                <span className="text-xs text-muted-foreground tabular-nums">
                  vs baseline: {signed(d.tdis_tot, 3)} K·h ({d.tdis_tot > 0 ? "less comfortable" : d.tdis_tot < 0 ? "more comfortable" : "same"})
                </span>
              )}
            </CardContent>
          </Card>
        </div>
        {ledger.measured ? (
          <div className="overflow-x-auto">
            <Table aria-label="End uses">
              <TableHeader>
                <TableRow>
                  <TableHead>End use</TableHead>
                  <TableHead>Scope</TableHead>
                  <TableHead className="text-right">kWh</TableHead>
                  <TableHead className="text-right">kgCO₂e</TableHead>
                  <TableHead className="text-right">$</TableHead>
                  {deltaById.size > 0 && <TableHead className="text-right">vs baseline (kgCO₂e)</TableHead>}
                </TableRow>
              </TableHeader>
              <TableBody>
                {ledger.end_uses.map((e) => {
                  const change = deltaById.get(e.id);
                  return (
                    <TableRow key={e.id}>
                      <TableCell className={e.measured ? undefined : "text-muted-foreground"}>{e.label}</TableCell>
                      <TableCell>
                        <Badge variant="outline">{e.scope}</Badge>
                      </TableCell>
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.kwh, 0)}</TableCell>
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.kg_co2e, 0)}</TableCell>
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.usd, 0)}</TableCell>
                      {deltaById.size > 0 && (
                        <TableCell className="text-right tabular-nums">{change ? signed(change.kg_co2e) : "—"}</TableCell>
                      )}
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
            <p className="mt-1 text-xs text-muted-foreground">
              End uses integrate the power readings recorded every 120 s; the rows marked as not in the samples are the
              rest of BOPTEST&apos;s exact total (its finer integration and the last step).
            </p>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            End uses not recorded: this run predates the fan and boiler power readings, so only the scope totals are
            shown.
          </p>
        )}
        <p className="border-t pt-3 text-xs text-muted-foreground">{LEDGER_FOOTER}</p>
      </CardContent>
    </Card>
  );
}
