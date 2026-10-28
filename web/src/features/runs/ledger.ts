import { formatNumber } from "@/lib/format";
import type { Amounts } from "@/types/generated/Amounts";
import type { Fault } from "@/types/generated/Fault";

import { faultModeText } from "./faults";

/** A change with its sign: "+12", "−848", "0" (a change that rounds to zero has none). */
export function signed(v: number, digits = 0): string {
  const text = formatNumber(Math.abs(v), digits);
  if (text === "0") return "0";
  return `${v > 0 ? "+" : "−"}${text}`;
}

/** A change in dollars: "+$12", "−$269". */
export function signedUsd(v: number): string {
  const text = formatNumber(Math.abs(v), 0);
  if (text === "0") return "$0";
  return `${v > 0 ? "+" : "−"}$${text}`;
}

/** A change as a percentage of `base`, or null when the base is zero. */
export function percentOf(change: number, base: number): number | null {
  return base === 0 ? null : (change / Math.abs(base)) * 100;
}

/** "+X kWh, +$Y, +Z kgCO₂e". */
export function changeText(d: Amounts): string {
  return `${signed(d.kwh)} kWh, ${signedUsd(d.usd)}, ${signed(d.kg_co2e)} kgCO₂e`;
}

/**
 * The fault banner: what was injected and, when the same run without the fault exists, what it
 * cost against that run. Without one, only the labelled fault.
 */
export function faultBanner(f: Fault, vsHealthy: { delta: Amounts; source: string } | null): string {
  const what = `Injected test fault: ${f.point} ${faultModeText(f)} from step ${f.from_step}`;
  return vsHealthy ? `${what}: ${changeText(vsHealthy.delta)} vs ${vsHealthy.source}` : `${what}.`;
}

export const LEDGER_FOOTER =
  "Simulation. Factors and prices are BOPTEST's for this test case (electricity 0.341, gas 0.181 kgCO₂/kWh; $0.094 and $0.024 per kWh). HVAC loads as BOPTEST counts them; lighting, plug loads and Scope 3 are not included.";
