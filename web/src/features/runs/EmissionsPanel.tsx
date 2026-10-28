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

import { digitsFor, endUsesMissing, faultBanner, ledgerFooter, money, percentOf, signed, signedMoney } from "./ledger";

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
  if (!hasKpis || ledger.status === "loading") return null;
  if (ledger.status === "error")
    return (
      <p className="flex items-start gap-2 text-sm text-muted-foreground">
        <TriangleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
        <span>
          No emissions ledger for this run (<code className="text-xs">{ledger.error.code}</code>): {ledger.error.message}
        </span>
      </p>
    );
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

function Change({ d, base, unit, digits }: { d: number; base: number; unit: string; digits: number }) {
  const pct = percentOf(d, base);
  return (
    <span className="text-xs text-muted-foreground tabular-nums">
      vs baseline: {signed(d, digits)} {unit}
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
  currency,
  digits,
  none,
}: {
  title: string;
  note: string;
  icon: React.ReactNode;
  a: Amounts;
  d: Amounts | null;
  base: Amounts | null;
  currency: string;
  digits: number;
  /** The scope does not apply to this building: say so instead of showing zeros. */
  none?: boolean;
}) {
  return (
    <Card className="py-4">
      <CardContent className="flex flex-col gap-1">
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          {icon}
          {title}
        </span>
        {none ? (
          <span className="text-2xl font-semibold">None</span>
        ) : (
          <>
            <span className="text-2xl font-semibold tabular-nums">{formatNumber(a.kg_co2e, digits)} kgCO₂e</span>
            <span className="text-sm tabular-nums">
              {formatNumber(a.kwh, digits)} kWh · {money(a.cost, currency, digits)}
            </span>
            {d && base && <Change d={d.kg_co2e} base={base.kg_co2e} unit="kg" digits={digits} />}
          </>
        )}
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
  const f = ledger.factors;
  const cur = f.currency;
  const digits = digitsFor(f);
  // An all-electric building burns nothing on site, and its prices may vary hour by hour.
  const allElectric = f.gas_kg_per_kwh === undefined;
  const spot = f.electricity_price_per_kwh === undefined;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Energy &amp; emissions</CardTitle>
        <CardDescription>
          {allElectric
            ? "All electric: Scope 2 (electricity bought) only, from BOPTEST's energy, emission and cost KPIs for this run."
            : "Scope 1 (gas burnt on site) and Scope 2 (electricity bought), split exactly from BOPTEST's energy and emission KPIs for this run."}
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
                cur,
                digits,
              )}
            </AlertDescription>
          </Alert>
        )}
        {!ledger.reconciles && (
          <p className="flex items-center gap-2 text-sm text-destructive">
            <TriangleAlert aria-hidden="true" className="size-4" />{" "}
            {spot
              ? "BOPTEST's emissions do not match its energy times the factor"
              : `The split does not re-price to BOPTEST's cost (${money(ledger.boptest_cost, cur, 2)})`}
            ; treat these numbers with care.
          </p>
        )}
        <div className="grid gap-3 sm:grid-cols-3">
          <ScopeCard
            title="Scope 1"
            note={allElectric ? "No fuel burnt on site: the heat pump runs on electricity" : "Gas: the boiler (zone reheat)"}
            icon={<Factory aria-hidden="true" className="size-4" />}
            a={ledger.scope1}
            d={d?.scope1 ?? null}
            base={base?.scope1 ?? null}
            currency={cur}
            digits={digits}
            none={allElectric}
          />
          <ScopeCard
            title="Scope 2"
            note={allElectric ? "Electricity: heat pump, its fan, the floor pump" : "Electricity: chillers, pumps, towers, fans"}
            icon={<Zap aria-hidden="true" className="size-4" />}
            a={ledger.scope2}
            d={d?.scope2 ?? null}
            base={base?.scope2 ?? null}
            currency={cur}
            digits={digits}
          />
          <Card className="py-4">
            <CardContent className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Total and comfort</span>
              <span className="text-2xl font-semibold tabular-nums">{money(ledger.total.cost, cur, digits)}</span>
              {d && <span className="text-xs text-muted-foreground tabular-nums">vs baseline: {signedMoney(d.total.cost, cur, digits)}</span>}
              {spot && (
                <span className="text-xs text-muted-foreground tabular-nums">
                  Average price paid {money(ledger.average_price_per_kwh, cur, 4)}/kWh
                  {base ? ` (baseline ${money(base.average_price_per_kwh, cur, 4)})` : ""}
                </span>
              )}
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
                  <TableHead className="text-right">{cur === "EUR" ? "€" : "$"}</TableHead>
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
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.kwh, digits)}</TableCell>
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.kg_co2e, digits)}</TableCell>
                      <TableCell className="text-right tabular-nums">{formatNumber(e.amounts.cost, digits)}</TableCell>
                      {deltaById.size > 0 && (
                        <TableCell className="text-right tabular-nums">{change ? signed(change.kg_co2e, digits) : "—"}</TableCell>
                      )}
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
            <p className="mt-1 text-xs text-muted-foreground">
              End uses integrate the power readings recorded every {ledger.subject.step_s} s; the rows marked as not in
              the samples are the rest of BOPTEST&apos;s exact total (its finer integration and the last step).
              {spot ? " With spot prices, each end use is priced at the run's average price paid." : ""}
            </p>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">{endUsesMissing(ledger)}</p>
        )}
        <p className="border-t pt-3 text-xs text-muted-foreground">{ledgerFooter(f)}</p>
      </CardContent>
    </Card>
  );
}
