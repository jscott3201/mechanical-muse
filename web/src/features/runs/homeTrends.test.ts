import { describe, expect, it } from "vitest";

import type { TickPoint } from "@/hooks/runStream";

import { FLEET_HOMES, HOME_POINTS, homeTrends, peakCut } from "./homeTrends";

const tick = (t: number, readings: Record<string, number>, writes: Record<string, number> = {}): TickPoint => ({
  seq: t,
  t,
  outputs: {},
  writes,
  readings,
});

describe("homeTrends", () => {
  it("converts the room and band to °C and sums the three powers to kW", () => {
    const tr = homeTrends([
      tick(
        0,
        {
          reaTZon_y: 294.15,
          [HOME_POINTS.floor]: 294.15,
          [HOME_POINTS.ceiling]: 297.15,
          [HOME_POINTS.price]: 0.25,
          reaPHeaPum_y: 1500,
          reaPFan_y: 300,
          reaPPumEmi_y: 200,
        },
        { oveTSet_u: 295.15 },
      ),
    ]);
    const at = (s: { points: { y: number | null }[] }[], id: number) => s[id]?.points[0]?.y;
    expect(at(tr.comfort, 0)).toBeCloseTo(21, 9);
    expect(at(tr.comfort, 1)).toBeCloseTo(22, 9);
    expect(at(tr.comfort, 3)).toBeCloseTo(24, 9);
    expect(at(tr.price, 0)).toBe(0.25);
    expect(at(tr.power, 0)).toBeCloseTo(2, 9);
  });

  it("leaves power as no data when a reading is missing, never 0", () => {
    const tr = homeTrends([tick(0, { reaPHeaPum_y: 1500, reaPFan_y: 300 })]);
    expect(tr.power[0]?.points[0]?.y).toBeNull();
  });
});

describe("peakCut", () => {
  const p = (peak_kw: number) => ({ threshold: 0.25, peak_kw, mean_kw: 1, peak_steps: 336, steps: 1344 });

  it("gives the cut per home and over the illustrative fleet", () => {
    const cut = peakCut(p(0.85), p(1.17));
    expect(cut?.perHomeKw).toBeCloseTo(0.32, 9);
    expect(cut?.percent).toBeCloseTo((0.32 / 1.17) * 100, 9);
    expect(cut?.fleetMw).toBeCloseTo((0.32 * FLEET_HOMES) / 1000, 9);
  });

  it("has no cut without a baseline figure", () => {
    expect(peakCut(p(1), null)).toBeNull();
    expect(peakCut(p(1), p(0))).toBeNull();
  });
});
