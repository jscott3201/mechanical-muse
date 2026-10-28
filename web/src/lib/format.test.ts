import { describe, expect, it } from "vitest";

import {
  formatNumber,
  formatWire,
  formatWithUnit,
  fractionToPercent,
  kelvinToCelsius,
  pascalsToKilopascals,
  wattsToKilowatts,
} from "./format";

describe("formatWithUnit", () => {
  it("adds Celsius and Fahrenheit to temperatures but not to temperature differences", () => {
    expect(formatWithUnit(278.71, "K")).toBe("278.71 K (5.56 °C, 42 °F)");
    expect(formatWithUnit(285.93, "K")).toBe("285.93 K (12.78 °C, 55 °F)");
    expect(formatWithUnit(3, "K")).toBe("3 K");
    expect(formatWithUnit(0.1, "K")).toBe("0.1 K");
  });

  it("shows pressures in kPa and never a non-finite value as a number", () => {
    expect(formatWithUnit(239125, "Pa")).toBe("239.125 kPa");
    expect(formatWithUnit(Number.NaN, "Pa")).toBe("no data");
  });

  it("formats wire values of every kind", () => {
    expect(formatWire(true)).toBe("true");
    expect(formatWire("NaN")).toBe("NaN");
    expect(formatWire({ class: 3, ordinal: 1 })).toBe("enum 3#1");
  });

  it("converts SI values for display", () => {
    expect(kelvinToCelsius(273.15)).toBe(0);
    expect(pascalsToKilopascals(45000)).toBe(45);
    expect(wattsToKilowatts(1500)).toBe(1.5);
    expect(fractionToPercent(0.25)).toBe(25);
  });

  it("never shows negative zero", () => {
    expect(formatNumber(-0.004, 2)).toBe("0");
    expect(formatNumber(-0.0001, 4)).toBe("-0.0001");
  });
});
