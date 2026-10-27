import { describe, expect, it } from "vitest";

import type { TickPoint } from "@/hooks/runStream";
import type { Binding } from "@/types/generated/Binding";

import { ownTrends } from "./trends";

const binding: Binding = {
  floors: [1, 2],
  inputs: [
    { input: "ena", kind: "above", point: "floor{floor}_reaAHU_occ_y", threshold: 0.5, value: 0 },
    { input: "oat", kind: "measurement", point: "weaSta_reaWeaTDryBul_y", threshold: 0, value: 0 },
    { input: "k", kind: "constant", point: "", threshold: 0, value: 3 },
  ],
  outputs: [
    { output: "sp", point: "floor{floor}_TSupAirSet", measured: "floor{floor}_reaAHU_TSup_y", min: 285, max: 300 },
    { output: "flag", point: "", measured: "", min: 0, max: 1 },
  ],
  notes: [],
};

const tick = (t: number): TickPoint => ({
  seq: t,
  t,
  outputs: {},
  writes: { floor1_TSupAirSet_u: 288.15, floor2_TSupAirSet_u: 289.15 },
  readings: { floor1_reaAHU_TSup_y: 288.65, floor2_reaAHU_TSup_y: 289.65, floor1_reaAHU_occ_y: 1, floor2_reaAHU_occ_y: 1, weaSta_reaWeaTDryBul_y: 297.15 },
});

describe("ownTrends", () => {
  it("charts each written output per floor and each measured input, in °C for kelvin", () => {
    const charts = ownTrends([tick(0), tick(120)], { sat: binding });
    expect(charts.map((c) => c.id)).toEqual(["sat-sp", "sat-ena", "sat-oat"]);
    const sp = charts[0];
    expect(sp?.unit).toBe("°C");
    expect(sp?.series.map((s) => s.label)).toEqual(["floor 1 sent", "floor 1 measured", "floor 2 sent", "floor 2 measured"]);
    expect(sp?.series[0]?.points[0]?.y).toBeCloseTo(15, 6);
    expect(sp?.series[0]?.dashed).toBe(true);
    const oat = charts[2];
    expect(oat?.series).toHaveLength(1);
    expect(oat?.series[0]?.points[0]?.y).toBeCloseTo(24, 6);
    expect(charts[1]?.unit).toBe("");
  });
});
