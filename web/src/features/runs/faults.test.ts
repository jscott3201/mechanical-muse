import { describe, expect, it } from "vitest";

import { endUsesMissing, faultBanner, signed, signedUsd } from "./ledger";
import { failSafeNote, faultModeText, faultRequest, faultText } from "./faults";

describe("endUsesMissing", () => {
  it("gives the actual reason end uses are not shown", () => {
    const old = ["floor1_reaAHU_PFanSup_y", "reaHotWatSys_reaPBoi_y", "floor2_reaAHU_PFanSup_y"];
    expect(endUsesMissing({ missing: old, subject: { ticks: 240 } })).toMatch(/predates the fan and boiler/);
    expect(endUsesMissing({ missing: ["floor2_reaAHU_PFreCoi_y"], subject: { ticks: 240 } })).toBe(
      "End uses not shown: floor2_reaAHU_PFreCoi_y missing or not a number in at least one tick, so only the scope totals are shown.",
    );
    expect(endUsesMissing({ missing: old, subject: { ticks: 1 } })).toMatch(/fewer than two ticks/);
  });
});

describe("faults", () => {
  it("builds the request for each kind", () => {
    expect(faultRequest("nan", 20)).toEqual({ point: "floor1_reaAHU_TSup_y", from_step: 20, steps: 5, mode: { kind: "nan" } });
    expect(faultRequest("hold", 30)).toEqual({
      point: "floor1_reaAHU_yCooVal_y",
      from_step: 30,
      steps: 60,
      mode: { kind: "hold", value: 1 },
    });
    expect(faultRequest("bias", 0).mode).toEqual({ kind: "bias", offset: 4 });
  });

  it("says what the fault did to the reading", () => {
    expect(faultText(faultRequest("hold", 30))).toBe("floor1_reaAHU_yCooVal_y held at 1 for steps 30–89");
    expect(faultModeText({ ...faultRequest("bias", 0), mode: { kind: "bias", offset: -1.5 } })).toBe("biased by −1.5");
    expect(faultText(faultRequest("nan", 20))).toBe("floor1_reaAHU_TSup_y reads NaN for steps 20–24");
  });

  it("notes the steps the fail-safe acted, after the fact", () => {
    expect(failSafeNote([])).toBeNull();
    expect(failSafeNote([{ from: 20, to: 24 }])).toMatch(/^Fail-safe active for steps 20–24: /);
    expect(failSafeNote([{ from: 7, to: 7 }])).toMatch(/^Fail-safe active for step 7: /);
    expect(failSafeNote([{ from: 1, to: 2 }, { from: 5, to: 5 }, { from: 9, to: 12 }])).toMatch(
      /^Fail-safe active for steps 1–2, 5 and 9–12: /,
    );
  });
});

describe("ledger text", () => {
  it("signs changes and leaves zero unsigned", () => {
    expect(signed(12.4)).toBe("+12");
    expect(signed(-848.2)).toBe("−848");
    expect(signed(-0.2)).toBe("0");
    expect(signedUsd(-268.56)).toBe("−$269");
  });

  it("prices a fault only against the same run without it", () => {
    const f = faultRequest("hold", 30);
    expect(faultBanner(f, { delta: { kwh: 412.3, kg_co2e: 140.6, usd: 38.8 }, source: "run run-3" })).toBe(
      "floor1_reaAHU_yCooVal_y held at 1 from step 30: +412 kWh, +$39, +141 kgCO₂e vs run run-3",
    );
    expect(faultBanner(f, null)).toBe("floor1_reaAHU_yCooVal_y held at 1 from step 30.");
  });
});
