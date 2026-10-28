import { ArrowDownRight, ArrowUpRight, Home, Minus } from "lucide-react";
import { useMemo } from "react";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { LineChart } from "@/components/viz/LineChart";
import type { TickPoint } from "@/hooks/runStream";
import { formatNumber } from "@/lib/format";
import type { PeakPower } from "@/types/generated/PeakPower";

import { FLEET_HOMES, homeTrends, peakCut } from "./homeTrends";
import { formatSimTime } from "./trends";

/** The home's charts: comfort, price and power, on BOPTEST simulation time. */
export function HomeCharts({ ticks }: { ticks: TickPoint[] }) {
  const t = useMemo(() => homeTrends(ticks), [ticks]);
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <LineChart title="Room temperature and comfort band" unit="°C" series={t.comfort} formatX={formatSimTime} formatY={(v) => formatNumber(v, 1)} />
      <LineChart title="Electric power (heat pump, fan, floor pump)" unit="kW" series={t.power} formatX={formatSimTime} formatY={(v) => formatNumber(v, 2)} />
      <LineChart title="Electricity price now, fees included" unit="€/kWh" series={t.price} step formatX={formatSimTime} formatY={(v) => formatNumber(v, 3)} />
    </div>
  );
}

/**
 * Power during the window's most expensive quarter of prices, against BOPTEST's own controller
 * over the same window, and the same cut over an illustrative fleet. Our figure from recorded
 * readings, not a BOPTEST KPI; the fleet line is a plain multiplication and says so.
 */
export function PeakPowerCard({
  peak,
  base,
  baseSource,
  baseline,
}: {
  peak: PeakPower;
  base: PeakPower | null;
  baseSource: string | null;
  /** This run is itself the baseline. */
  baseline: boolean;
}) {
  const cut = peakCut(peak, base);
  const Icon = cut === null || Math.abs(cut.percent) < 0.05 ? Minus : cut.perHomeKw > 0 ? ArrowDownRight : ArrowUpRight;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Home aria-hidden="true" className="size-4" /> Power in peak-price hours
        </CardTitle>
        <CardDescription>
          Mean electric power in this window's most expensive quarter of 15-minute prices ({peak.peak_steps} of {peak.steps} steps,
          {" "}€{formatNumber(peak.threshold, 3)}/kWh and up). Worked out from the recorded readings, not a BOPTEST KPI.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">{baseline ? "BOPTEST's controller" : "This run"}</span>
            <span className="text-2xl font-semibold tabular-nums">{formatNumber(peak.peak_kw, 2)} kW</span>
            <span className="text-xs text-muted-foreground">mean over all hours {formatNumber(peak.mean_kw, 2)} kW</span>
          </div>
          {base && (
            <div className="flex flex-col">
              <span className="text-xs text-muted-foreground">BOPTEST's controller</span>
              <span className="text-2xl font-semibold tabular-nums">{formatNumber(base.peak_kw, 2)} kW</span>
              <span className="text-xs text-muted-foreground">{baseSource}</span>
            </div>
          )}
          {cut && (
            <div className="flex flex-col">
              <span className="text-xs text-muted-foreground">Change in peak-price hours</span>
              <span className="flex items-center gap-1 text-2xl font-semibold tabular-nums">
                <Icon aria-hidden="true" className="size-5 text-muted-foreground" />
                {formatNumber(-cut.percent, 0)} %
              </span>
              <span className="text-xs text-muted-foreground">{formatNumber(cut.perHomeKw, 2)} kW less per home</span>
            </div>
          )}
        </div>
        {cut && cut.perHomeKw > 0 && (
          <p className="text-sm">
            <span className="font-medium">Fleet view (an illustration):</span> {FLEET_HOMES.toLocaleString("en-US")} homes like this one,
            each cutting {formatNumber(cut.perHomeKw, 2)} kW, would draw about {formatNumber(cut.fleetMw, 1)} MW less in these hours. A
            plain multiplication of one simulated house in Brussels: real fleets differ in homes, weather and who takes part.
          </p>
        )}
        {!baseline && !base && (
          <p className="text-sm text-muted-foreground">
            No baseline of the same window yet: run one (Baseline only) to compare.
          </p>
        )}
      </CardContent>
    </Card>
  );
}
