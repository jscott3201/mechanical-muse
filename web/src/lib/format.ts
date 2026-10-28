/** Display helpers. Data stays in SI (K, Pa); conversion happens here only, for display. */
import type { ScenarioResult } from "@/types/generated/ScenarioResult";

/** An engine value as it crosses the wire (number, boolean, non-finite real as text, or enum). */
export type WireValue = Extract<ScenarioResult, { result: "fail" }>["expected"];

/** A temperature in kelvin as Celsius. */
export function kelvinToCelsius(k: number): number {
  return k - 273.15;
}

/** Pascals as kilopascals. */
export function pascalsToKilopascals(pa: number): number {
  return pa / 1000;
}

/** Watts as kilowatts. */
export function wattsToKilowatts(w: number): number {
  return w / 1000;
}

/** A fraction (0 to 1, as BOPTEST reports valve positions) as a percentage. */
export function fractionToPercent(f: number): number {
  return f * 100;
}

export function formatNumber(value: number, digits = 4): string {
  if (!Number.isFinite(value)) return "no data";
  const text = value.toLocaleString("en-US", { maximumFractionDigits: digits });
  // A small negative that rounds to zero would read "-0".
  return text === "-0" ? "0" : text;
}

/** A temperature in kelvin as Fahrenheit, for the engineers and sheets that think in °F. */
export function kelvinToFahrenheit(k: number): number {
  return kelvinToCelsius(k) * 1.8 + 32;
}

/** A value with its unit, adding °C and °F beside kelvin and kPa beside pascals. */
export function formatWithUnit(value: number, unit: string): string {
  if (!Number.isFinite(value)) return "no data";
  switch (unit) {
    case "K":
      // Kelvin carries both temperatures and temperature differences (an error of 3 K). Only
      // values in the range of real temperatures get Celsius and Fahrenheit beside them.
      return value > 150
        ? `${formatNumber(value, 2)} K (${formatNumber(kelvinToCelsius(value), 2)} °C, ${formatNumber(kelvinToFahrenheit(value), 1)} °F)`
        : `${formatNumber(value, 3)} K`;
    case "Pa":
      return Math.abs(value) >= 1000
        ? `${formatNumber(pascalsToKilopascals(value), 3)} kPa`
        : `${formatNumber(value, 1)} Pa`;
    case "s":
      return `${formatNumber(value, 1)} s`;
    case "1":
    case "":
      return formatNumber(value);
    default:
      return `${formatNumber(value)} ${unit}`;
  }
}

/** A wire value (number, boolean, string, or enumeration member) as text. */
export function formatWire(value: WireValue): string {
  if (typeof value === "number") return formatNumber(value, 6);
  if (typeof value === "boolean") return value ? "true" : "false";
  // Engine values have no text type here: a string is a non-finite real ("NaN", "Infinity"),
  // shown as the engine reported it so a receipt says exactly what was compared.
  if (typeof value === "string") return value;
  return `enum ${value.class}#${value.ordinal}`;
}
