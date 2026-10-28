import { formatNumber } from "@/lib/format";
import type { Amounts } from "@/types/generated/Amounts";
import type { Fault } from "@/types/generated/Fault";
import type { LedgerFactors } from "@/types/generated/LedgerFactors";

import { faultModeText } from "./faults";

/** A change with its sign: "+12", "−848", "0" (a change that rounds to zero has none). */
export function signed(v: number, digits = 0): string {
  const text = formatNumber(Math.abs(v), digits);
  if (Number(text.replace(/,/g, "")) === 0) return "0";
  return `${v > 0 ? "+" : "−"}${text}`;
}

/** The symbol of a ledger's currency (BOPTEST's own for the test case). */
export function currencySymbol(currency: string): string {
  return currency === "EUR" ? "€" : currency === "USD" ? "$" : `${currency} `;
}

/** Decimals for a ledger's amounts: whole numbers for the office, one for a house (its totals
 * are tens of kWh and kg, not thousands). */
export function digitsFor(f: Pick<LedgerFactors, "area_m2">): number {
  return f.area_m2 < 1000 ? 1 : 0;
}

/** An amount of money: "$963", "€72.6". */
export function money(v: number, currency: string, digits = 0): string {
  return `${currencySymbol(currency)}${formatNumber(v, digits)}`;
}

/** A change in money: "+$12", "−$269", "−€6.6". */
export function signedMoney(v: number, currency: string, digits = 0): string {
  const text = formatNumber(Math.abs(v), digits);
  const sym = currencySymbol(currency);
  if (Number(text.replace(/,/g, "")) === 0) return `${sym}0`;
  return `${v > 0 ? "+" : "−"}${sym}${text}`;
}

/** A change in dollars: "+$12", "−$269". */
export function signedUsd(v: number): string {
  return signedMoney(v, "USD");
}

/** A change as a percentage of `base`, or null when the base is zero. */
export function percentOf(change: number, base: number): number | null {
  return base === 0 ? null : (change / Math.abs(base)) * 100;
}

/** "+X kWh, +$Y, +Z kgCO₂e". */
export function changeText(d: Amounts, currency = "USD", digits = 0): string {
  return `${signed(d.kwh, digits)} kWh, ${signedMoney(d.cost, currency, digits)}, ${signed(d.kg_co2e, digits)} kgCO₂e`;
}

/**
 * The fault banner: what was injected and, when the same run without the fault exists, what it
 * cost against that run. Without one, only the labelled fault.
 */
export function faultBanner(
  f: Fault,
  vsHealthy: { delta: Amounts; source: string } | null,
  currency = "USD",
  digits = 0,
): string {
  const what = `${f.point} ${faultModeText(f)} from step ${f.from_step}`;
  return vsHealthy ? `${what}: ${changeText(vsHealthy.delta, currency, digits)} vs ${vsHealthy.source}` : `${what}.`;
}

/** Why a ledger has no end uses, from the signals its record lacks. */
export function endUsesMissing(l: { missing: string[]; subject: { ticks: number } }): string {
  const older = ["reaHotWatSys_reaPBoi_y", "floor1_reaAHU_PFanSup_y"];
  if (l.subject.ticks < 2) return "End uses not recorded: the run has fewer than two ticks to integrate.";
  if (older.every((p) => l.missing.includes(p)))
    return "End uses not recorded: this run predates the fan and boiler power readings, so only the scope totals are shown.";
  const shown = l.missing.slice(0, 3).join(", ");
  const more = l.missing.length > 3 ? ` and ${l.missing.length - 3} more` : "";
  return `End uses not shown: ${shown}${more} missing or not a number in at least one tick, so only the scope totals are shown.`;
}

/** The footer: the factors and prices every number used, from the ledger itself. */
export function ledgerFooter(f: LedgerFactors): string {
  const sym = currencySymbol(f.currency);
  const factors =
    f.gas_kg_per_kwh === undefined
      ? `electricity ${formatNumber(f.electricity_kg_per_kwh, 3)} kgCO₂/kWh, the same every hour`
      : `electricity ${formatNumber(f.electricity_kg_per_kwh, 3)}, gas ${formatNumber(f.gas_kg_per_kwh, 3)} kgCO₂/kWh`;
  const prices =
    f.electricity_price_per_kwh === undefined
      ? "spot electricity prices, as BOPTEST charges them"
      : `${sym}${formatNumber(f.electricity_price_per_kwh, 3)}${f.gas_price_per_kwh === undefined ? "" : ` and ${sym}${formatNumber(f.gas_price_per_kwh, 3)}`} per kWh`;
  const loads =
    f.gas_kg_per_kwh === undefined
      ? "The house's heating as BOPTEST counts it (heat pump, its fan, the floor pump)"
      : "HVAC loads as BOPTEST counts them";
  return `Simulation. Factors and prices are BOPTEST's for this test case (${factors}; ${prices}). ${loads}; lighting, plug loads and Scope 3 are not included.`;
}
