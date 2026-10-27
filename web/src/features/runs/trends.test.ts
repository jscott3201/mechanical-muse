import { describe, expect, it } from "vitest";

import type { TickPoint } from "@/hooks/runStream";

import { formatSimTime, trends } from "./trends";

const tick = (t: number, over: Partial<TickPoint> = {}): TickPoint => ({
  seq: 0,
  t,
  outputs: {
    ahu1: { yChiWatResReq: 1 },
    ahu2: { yChiWatResReq: 0 },
    ahu3: { yChiWatResReq: 2 },
  },
  writes: { oveChiWatSys_TWSet_u: 278.71, oveChiWatSys_dpSet_u: 239125 },
  readings: {
    reaChiWatSys_TCHWSup_y: 279.15,
    reaChiWatSys_dp_y: 100000,
    reaChiWatSys_reaPChi_y: 452000,
    floor1_reaAHU_yCooVal_y: 0.4,
    floor2_reaAHU_yCooVal_y: 0.97,
  },
  ...over,
});

describe("trends", () => {
  it("converts units for display only and sums the requests", () => {
    const tr = trends([tick(16027200)]);
    expect(tr.supplyTemperature[0]?.points[0]?.y).toBeCloseTo(5.56, 6);
    expect(tr.supplyTemperature[1]?.points[0]?.y).toBeCloseTo(6.0, 6);
    expect(tr.pressure[0]?.points[0]?.y).toBeCloseTo(239.125, 6);
    expect(tr.requests[0]?.points[0]?.y).toBe(3);
    expect(tr.valves.map((s) => s.points[0]?.y)).toEqual([40, 97, null]);
    expect(tr.chillerPower[0]?.points[0]?.y).toBe(452);
  });

  it("never turns missing data into zero", () => {
    const tr = trends([tick(0, { writes: {}, readings: {}, outputs: {} })]);
    expect(tr.supplyTemperature.every((s) => s.points[0]?.y === null)).toBe(true);
    expect(tr.requests[0]?.points[0]?.y).toBeNull();
    expect(tr.chillerPower[0]?.points[0]?.y).toBeNull();
  });

  it("skips ticks without a simulation time", () => {
    expect(trends([tick(0, { t: null })]).requests[0]?.points).toHaveLength(0);
  });
});

describe("formatSimTime", () => {
  it("shows day of year and clock time", () => {
    expect(formatSimTime(185 * 86400 + 12 * 3600 + 30 * 60)).toBe("day 185 12:30");
  });
});
