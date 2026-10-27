/**
 * Ticks of a run -> chart series. This is the one place that knows the demo's BOPTEST point
 * names (by their ids) and converts units for display (K -> °C, Pa -> kPa, W -> kW).
 */
import type { Series } from "@/components/viz/LineChart";
import type { TickPoint } from "@/hooks/runStream";

export const POINTS = {
  tsupSet: "oveChiWatSys_TWSet_u",
  tsup: "reaChiWatSys_TCHWSup_y",
  dpSet: "oveChiWatSys_dpSet_u",
  dp: "reaChiWatSys_dp_y",
  chillers: "reaChiWatSys_reaPChi_y",
  valve: (floor: number) => `floor${floor}_reaAHU_yCooVal_y`,
} as const;

export const FLOORS = [1, 2, 3] as const;

export interface Trends {
  supplyTemperature: Series[];
  pressure: Series[];
  requests: Series[];
  valves: Series[];
  chillerPower: Series[];
}

type Pick = (t: TickPoint) => number | null;

function series(id: string, label: string, color: string, ticks: TickPoint[], pick: Pick, dashed = false): Series {
  const points = ticks.filter((t) => t.t !== null).map((t) => ({ x: t.t as number, y: pick(t) }));
  return { id, label, color, dashed, points };
}

const num = (v: number | undefined): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);

/** Requests summed over the air handlers; null when no handler reported this tick. */
function totalRequests(t: TickPoint): number | null {
  let total = 0;
  let seen = false;
  for (const f of FLOORS) {
    const v = t.outputs[`ahu${f}`]?.["yChiWatResReq"];
    if (typeof v === "number") {
      total += v;
      seen = true;
    }
  }
  return seen ? total : null;
}

export function trends(ticks: TickPoint[]): Trends {
  const c = (k: number) => num(k) === null ? null : k - 273.15;
  return {
    supplyTemperature: [
      series("set", "setpoint sent", "var(--viz-1)", ticks, (t) => {
        const v = num(t.writes[POINTS.tsupSet]);
        return v === null ? null : c(v);
      }, true),
      series("meas", "measured", "var(--viz-2)", ticks, (t) => {
        const v = num(t.readings[POINTS.tsup]);
        return v === null ? null : c(v);
      }),
    ],
    pressure: [
      series("set", "setpoint sent", "var(--viz-1)", ticks, (t) => {
        const v = num(t.writes[POINTS.dpSet]);
        return v === null ? null : v / 1000;
      }, true),
      series("meas", "measured", "var(--viz-2)", ticks, (t) => {
        const v = num(t.readings[POINTS.dp]);
        return v === null ? null : v / 1000;
      }),
    ],
    requests: [series("req", "requests", "var(--viz-1)", ticks, totalRequests)],
    valves: FLOORS.map((f, i) =>
      series(`f${f}`, `floor ${f}`, `var(--viz-${i + 1})`, ticks, (t) => {
        const v = num(t.readings[POINTS.valve(f)]);
        return v === null ? null : v * 100;
      }),
    ),
    chillerPower: [
      series("chi", "chillers", "var(--viz-1)", ticks, (t) => {
        const v = num(t.readings[POINTS.chillers]);
        return v === null ? null : v / 1000;
      }),
    ],
  };
}

/** BOPTEST time as whole days since January 1 and clock time ("day 185 14:30"). */
export function formatSimTime(s: number): string {
  const total = Math.round(s / 60);
  const day = Math.floor(total / 1440);
  const hh = String(Math.floor((total % 1440) / 60)).padStart(2, "0");
  const mm = String(total % 60).padStart(2, "0");
  return `day ${day} ${hh}:${mm}`;
}
