import type { Fault } from "@/types/generated/Fault";

/**
 * The test faults the run form offers. Each changes one reading the sequences see. Nothing is
 * written to BOPTEST for that reading; the setpoints the sequences compute from it still are,
 * within their clamps.
 */
export type FaultKind = Fault["mode"]["kind"];

/** `unit` is the reading's BOPTEST unit, shown with a bias ("+4 K"); empty when it has none. */
type Choice = { kind: FaultKind; label: string; detail: string; point: string; unit: string; steps: number };

export const FAULT_CHOICES: readonly Choice[] = [
  {
    kind: "nan",
    label: "Blank reading",
    point: "floor1_reaAHU_TSup_y",
    unit: "K",
    steps: 5,
    detail:
      "Floor 1's supply-air temperature reads NaN for 5 steps. The sequences that use it stop writing and BOPTEST's own controllers take over, then they resume.",
  },
  {
    kind: "hold",
    label: "Stuck valve reading",
    point: "floor1_reaAHU_yCooVal_y",
    unit: "",
    steps: 60,
    detail:
      "Floor 1's cooling-valve reading is held at 100 % open for 60 steps, so its air handler keeps asking for colder water. The plant reset ignores the first two requests, so it reacts when other floors are asking too. Only the reading is faulted; the setpoints computed from it still reach BOPTEST, within their clamps.",
  },
  {
    kind: "bias",
    label: "Biased reading",
    point: "floor1_reaAHU_TSup_y",
    unit: "K",
    steps: 60,
    detail:
      "Floor 1's supply-air temperature reads 4 K high for 60 steps, as a drifted sensor would. Its air handler then sends 3 requests, more than the plant reset ignores, so the plant is driven colder than it needs to be. Only the reading is faulted; the setpoints computed from it still reach BOPTEST, within their clamps.",
  },
];

/** The fault a run request carries for `kind`, from step `fromStep`. */
export function faultRequest(kind: FaultKind, fromStep: number): Fault {
  const c = FAULT_CHOICES.find((x) => x.kind === kind) ?? (FAULT_CHOICES[0] as Choice);
  const mode: Fault["mode"] =
    c.kind === "hold" ? { kind: "hold", value: 1 } : c.kind === "bias" ? { kind: "bias", offset: 4 } : { kind: "nan" };
  return { point: c.point, from_step: fromStep, steps: c.steps, mode };
}

/** The unit of a faulted reading, for the points the run form offers. */
function readingUnit(point: string): string {
  const unit = FAULT_CHOICES.find((c) => c.point === point)?.unit ?? "";
  return unit ? ` ${unit}` : "";
}

/** What the fault did to the reading ("reads NaN", "held at 1", "biased by +4 K"). */
export function faultModeText(f: Fault): string {
  // Records from before modes carry none; the server fills in NaN, but be safe.
  const mode: Fault["mode"] | undefined = f.mode as Fault["mode"] | undefined;
  switch (mode?.kind) {
    case "hold":
      return `held at ${mode.value}`;
    case "bias":
      return `biased by ${mode.offset >= 0 ? "+" : "−"}${Math.abs(mode.offset)}${readingUnit(f.point)}`;
    default:
      return "reads NaN";
  }
}

/**
 * "Fail-safe active for steps 20–24 and 40: ..." for the spans a run recorded, or null when the
 * fail-safe never acted. Step numbers count from 0, as the fault's do.
 */
export function failSafeNote(spans: readonly { from: number; to: number }[]): string | null {
  if (spans.length === 0) return null;
  const parts = spans.map((s) => (s.from === s.to ? `${s.from}` : `${s.from}–${s.to}`));
  const list = parts.length === 1 ? parts.join("") : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
  const one = spans.length === 1 && spans[0]?.from === spans[0]?.to;
  return `Fail-safe active for step${one ? "" : "s"} ${list}: no new setpoints were written for the affected sequences (last good values held or handed back to BOPTEST's own controllers).`;
}

/** "floor1_reaAHU_yCooVal_y held at 1 for steps 30–89". */
export function faultText(f: Fault): string {
  return `${f.point} ${faultModeText(f)} for steps ${f.from_step}–${f.from_step + f.steps - 1}`;
}
