/**
 * Ticks of a home run -> chart series. The one place that knows the home's BOPTEST point names
 * and the runtime's forecast reading names (`forecast:<point>:+<h>h`, see mm-runtime `home.rs`).
 */
import type { Series } from "@/components/viz/LineChart";
import type { TickPoint } from "@/hooks/runStream";
import { kelvinToCelsius, wattsToKilowatts } from "@/lib/format";
import type { PeakPower } from "@/types/generated/PeakPower";

export const HOME_POINTS = {
  room: "reaTZon_y",
  setpointSent: "oveTSet_u",
  floor: "forecast:LowerSetp[1]:+0h",
  ceiling: "forecast:UpperSetp[1]:+0h",
  price: "forecast:PriceElectricPowerHighlyDynamic:+0h",
  power: ["reaPHeaPum_y", "reaPFan_y", "reaPPumEmi_y"],
} as const;

export interface HomeTrends {
  /** Room temperature with the comfort band and the setpoint sent (°C). */
  comfort: Series[];
  /** Spot price now, including fixed fees (per kWh). */
  price: Series[];
  /** Heat pump, its fan and the floor pump together (kW). */
  power: Series[];
}

const num = (v: number | undefined): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);
const celsius = (v: number | undefined) => {
  const n = num(v);
  return n === null ? null : kelvinToCelsius(n);
};

function series(id: string, label: string, color: string, ticks: TickPoint[], pick: (t: TickPoint) => number | null, dashed = false): Series {
  const points = ticks.filter((t) => t.t !== null).map((t) => ({ x: t.t as number, y: pick(t) }));
  return { id, label, color, dashed, points };
}

/** The house's electric power, or null when any of the three readings is missing. */
function powerKw(t: TickPoint): number | null {
  let total = 0;
  for (const p of HOME_POINTS.power) {
    const v = num(t.readings[p]);
    if (v === null) return null;
    total += v;
  }
  return wattsToKilowatts(total);
}

export function homeTrends(ticks: TickPoint[]): HomeTrends {
  return {
    comfort: [
      series("room", "room", "var(--viz-2)", ticks, (t) => celsius(t.readings[HOME_POINTS.room])),
      series("set", "setpoint sent", "var(--viz-1)", ticks, (t) => celsius(t.writes[HOME_POINTS.setpointSent]), true),
      series("floor", "comfort floor", "var(--viz-3)", ticks, (t) => celsius(t.readings[HOME_POINTS.floor]), true),
      series("ceiling", "comfort ceiling", "var(--viz-3)", ticks, (t) => celsius(t.readings[HOME_POINTS.ceiling]), true),
    ],
    price: [series("price", "price now", "var(--viz-1)", ticks, (t) => num(t.readings[HOME_POINTS.price]))],
    power: [series("power", "house total", "var(--viz-2)", ticks, powerKw)],
  };
}

/** Homes in the illustrative fleet view: a round number, said to be an illustration. */
export const FLEET_HOMES = 10_000;

/**
 * The cut in mean power during the most expensive quarter of prices, against the baseline of the
 * same window (same prices, so the same quarter): per home (kW) and for the illustrative fleet
 * (MW, a plain multiplication). Null without a baseline figure.
 */
export function peakCut(run: PeakPower, base: PeakPower | undefined | null): { perHomeKw: number; percent: number; fleetMw: number } | null {
  if (!base || base.peak_kw <= 0) return null;
  const perHomeKw = base.peak_kw - run.peak_kw;
  return {
    perHomeKw,
    percent: (perHomeKw / base.peak_kw) * 100,
    fleetMw: (perHomeKw * FLEET_HOMES) / 1000,
  };
}
