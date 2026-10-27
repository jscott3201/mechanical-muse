/** Display helpers. Data stays in SI (K, Pa); conversion happens here only, for display. */

export function formatNumber(value: number, digits = 4): string {
  if (!Number.isFinite(value)) return "no data";
  return value.toLocaleString("en-US", { maximumFractionDigits: digits });
}

/** A value with its unit, adding °C beside kelvin and kPa beside pascals. */
export function formatWithUnit(value: number, unit: string): string {
  if (!Number.isFinite(value)) return "no data";
  switch (unit) {
    case "K":
      // Kelvin carries both temperatures and temperature differences (an error of 3 K). Only
      // values in the range of real temperatures get a Celsius reading beside them.
      return value > 150
        ? `${formatNumber(value, 2)} K (${formatNumber(value - 273.15, 2)} °C)`
        : `${formatNumber(value, 3)} K`;
    case "Pa":
      return value >= 1000
        ? `${formatNumber(value / 1000, 3)} kPa`
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
export function formatWire(
  value: number | boolean | string | { class: number; ordinal: number },
): string {
  if (typeof value === "number") return formatNumber(value, 6);
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "string") return value;
  return `enum ${value.class}#${value.ordinal}`;
}
