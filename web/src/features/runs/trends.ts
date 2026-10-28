/**
 * Ticks of a run -> chart series. This is the one place that knows the demo's BOPTEST point
 * names (by their ids) and which display unit each chart takes (K -> °C, Pa -> kPa, W -> kW);
 * the conversions themselves live in `lib/format`.
 */
import type { Series } from "@/components/viz/LineChart";
import type { TickPoint } from "@/hooks/runStream";
import { fractionToPercent, kelvinToCelsius, pascalsToKilopascals, wattsToKilowatts } from "@/lib/format";
import type { Binding } from "@/types/generated/Binding";

export const POINTS = {
  tsupSet: "oveChiWatSys_TWSet_u",
  tsup: "reaChiWatSys_TCHWSup_y",
  dpSet: "oveChiWatSys_dpSet_u",
  dp: "reaChiWatSys_dp_y",
  chillers: "reaChiWatSys_reaPChi_y",
  valve: (floor: number) => `floor${floor}_reaAHU_yCooVal_y`,
  ductSet: (floor: number) => `floor${floor}_dpSet_u`,
  fan: (floor: number) => `floor${floor}_reaAHU_PFanSup_y`,
} as const;

/** BOPTEST models floor 2 as ten identical floors: its flows and power are ten times one floor. */
const floorLabel = (f: number) => (f === 2 ? "floor 2 ×10" : `floor ${f}`);

export const FLOORS = [1, 2, 3] as const;

export interface Trends {
  supplyTemperature: Series[];
  pressure: Series[];
  requests: Series[];
  valves: Series[];
  chillerPower: Series[];
  /** Static pressure setpoints sent, per floor (runs with the pressure resets). */
  ductSetpoints: Series[];
  /** Supply fan power, per floor. */
  fanPower: Series[];
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

/** A conversion applied to a value only when there is one: a missing value stays missing. */
const conv = (to: (v: number) => number) => (v: number | undefined): number | null => {
  const n = num(v);
  return n === null ? null : to(n);
};
const celsius = conv(kelvinToCelsius);
const kpa = conv(pascalsToKilopascals);
const kw = conv(wattsToKilowatts);
const percent = conv(fractionToPercent);

export function trends(ticks: TickPoint[]): Trends {
  return {
    supplyTemperature: [
      series("set", "setpoint sent", "var(--viz-1)", ticks, (t) => celsius(t.writes[POINTS.tsupSet]), true),
      series("meas", "measured", "var(--viz-2)", ticks, (t) => celsius(t.readings[POINTS.tsup])),
    ],
    pressure: [
      series("set", "setpoint sent", "var(--viz-1)", ticks, (t) => kpa(t.writes[POINTS.dpSet]), true),
      series("meas", "measured", "var(--viz-2)", ticks, (t) => kpa(t.readings[POINTS.dp])),
    ],
    requests: [series("req", "requests", "var(--viz-1)", ticks, totalRequests)],
    valves: FLOORS.map((f, i) =>
      series(`f${f}`, `floor ${f}`, `var(--viz-${i + 1})`, ticks, (t) => percent(t.readings[POINTS.valve(f)])),
    ),
    chillerPower: [series("chi", "chillers", "var(--viz-1)", ticks, (t) => kw(t.readings[POINTS.chillers]))],
    ductSetpoints: FLOORS.map((f, i) =>
      series(`f${f}`, floorLabel(f), `var(--viz-${i + 1})`, ticks, (t) => num(t.writes[POINTS.ductSet(f)]), true),
    ),
    fanPower: FLOORS.map((f, i) =>
      series(`f${f}`, floorLabel(f), `var(--viz-${i + 1})`, ticks, (t) => kw(t.readings[POINTS.fan(f)])),
    ),
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

/** A chart of an engineer's own sequence: one bound output or measured input across floors. */
export interface OwnChart {
  id: string;
  title: string;
  unit: string;
  series: Series[];
}

const onFloor = (template: string, f: number) => template.replaceAll("{floor}", String(f));

/**
 * BOPTEST names temperature points with a capital T after the prefix (`TSupAirSet_u`,
 * `reaAHU_TSup_y`, `reaWeaTDryBul_y`), all in kelvin. Used only for records from before bindings
 * carried units, so their setpoint charts are not shown in kelvin with an empty unit.
 */
const KELVIN_NAME = /(^|_)(rea[A-Za-z]*?)?T[A-Z][A-Za-z]*(_[uy])?$/;

/** The unit to show: kelvin becomes °C, a dimensionless "1" shows none. */
function displayUnit(unit: string): string {
  return unit === "K" ? "°C" : unit === "1" ? "" : unit;
}

/**
 * Charts for engineers' own sequences, from their confirmed bindings: each written output as
 * the setpoint sent (dashed) beside the measurement that shows its result, per floor; each
 * measured input as read. Points the binding lists in kelvin show in °C; others as recorded.
 */
export function ownTrends(ticks: TickPoint[], bindings: Record<string, Binding>): OwnChart[] {
  const charts: OwnChart[] = [];
  for (const [spec, b] of Object.entries(bindings)) {
    // A record without units (older bindings) leaves the field out despite its type.
    const units: Binding["units"] | undefined = b.units as Binding["units"] | undefined;
    const unitOf = (template: string) => units?.[template] ?? (units === undefined && KELVIN_NAME.test(template) ? "K" : "");
    for (const o of b.outputs.filter((o) => o.point !== "")) {
      const k = unitOf(`${o.point}_u`) === "K";
      const show = (v: number | null) => (v === null ? null : k ? kelvinToCelsius(v) : v);
      charts.push({
        id: `${spec}-${o.output}`,
        title: `${o.output} (${spec})`,
        unit: displayUnit(unitOf(`${o.point}_u`)),
        series: b.floors.flatMap((f, i) => [
          series(`set${f}`, `floor ${f} sent`, `var(--viz-${i + 1})`, ticks, (t) => show(num(t.writes[`${onFloor(o.point, f)}_u`])), true),
          ...(o.measured
            ? [series(`meas${f}`, `floor ${f} measured`, `var(--viz-${i + 1})`, ticks, (t) => show(num(t.readings[onFloor(o.measured, f)])))]
            : []),
        ]),
      });
    }
    for (const input of b.inputs.filter((i) => i.kind !== "constant")) {
      const floors = input.point.includes("{floor}") ? b.floors : [b.floors[0] ?? 1];
      const k = unitOf(input.point) === "K";
      charts.push({
        id: `${spec}-${input.input}`,
        title: `${input.input} (${spec}, read from ${input.point})`,
        unit: displayUnit(unitOf(input.point)),
        series: floors.map((f, i) =>
          series(`in${f}`, floors.length > 1 ? `floor ${f}` : "read", `var(--viz-${i + 1})`, ticks, (t) => {
            const v = num(t.readings[onFloor(input.point, f)]);
            return v === null ? null : k ? kelvinToCelsius(v) : v;
          }),
        ),
      });
    }
  }
  return charts;
}
