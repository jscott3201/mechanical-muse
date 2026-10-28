import type { Fault } from "@/types/generated/Fault";

/**
 * The test faults the run form offers. Each changes one reading the sequences see. Nothing is
 * written to BOPTEST for that reading; the setpoints the sequences compute from it still are,
 * within their clamps.
 */
export type FaultKind = Fault["mode"]["kind"];

type Choice = { kind: FaultKind; label: string; detail: string; point: string; steps: number };

export const FAULT_CHOICES: readonly Choice[] = [
  {
    kind: "nan",
    label: "Blank reading",
    point: "floor1_reaAHU_TSup_y",
    steps: 5,
    detail:
      "Floor 1's supply-air temperature reads NaN for 5 steps. The sequences that use it stop writing and BOPTEST's own controllers take over, then they resume.",
  },
  {
    kind: "hold",
    label: "Stuck valve reading",
    point: "floor1_reaAHU_yCooVal_y",
    steps: 60,
    detail:
      "Floor 1's cooling-valve reading is held at 100 % open for 60 steps, so its air handler keeps asking for colder water. The plant reset ignores the first two requests, so it reacts when other floors are asking too. Only the reading is faulted; the setpoints computed from it still reach BOPTEST, within their clamps.",
  },
  {
    kind: "bias",
    label: "Biased reading",
    point: "floor1_reaAHU_TSup_y",
    steps: 60,
    detail:
      "Floor 1's supply-air temperature reads 2 K high for 60 steps, as a drifted sensor would. Only the reading is faulted; the setpoints computed from it still reach BOPTEST, within their clamps.",
  },
];

/** The fault a run request carries for `kind`, from step `fromStep`. */
export function faultRequest(kind: FaultKind, fromStep: number): Fault {
  const c = FAULT_CHOICES.find((x) => x.kind === kind) ?? (FAULT_CHOICES[0] as Choice);
  const mode: Fault["mode"] =
    c.kind === "hold" ? { kind: "hold", value: 1 } : c.kind === "bias" ? { kind: "bias", offset: 2 } : { kind: "nan" };
  return { point: c.point, from_step: fromStep, steps: c.steps, mode };
}

/** What the fault did to the reading ("reads NaN", "held at 1", "biased by +2"). */
export function faultModeText(f: Fault): string {
  // Records from before modes carry none; the server fills in NaN, but be safe.
  const mode: Fault["mode"] | undefined = f.mode as Fault["mode"] | undefined;
  switch (mode?.kind) {
    case "hold":
      return `held at ${mode.value}`;
    case "bias":
      return `biased by ${mode.offset >= 0 ? "+" : "−"}${Math.abs(mode.offset)}`;
    default:
      return "reads NaN";
  }
}

/** "floor1_reaAHU_yCooVal_y held at 1 for steps 30–89". */
export function faultText(f: Fault): string {
  return `${f.point} ${faultModeText(f)} for steps ${f.from_step}–${f.from_step + f.steps - 1}`;
}
