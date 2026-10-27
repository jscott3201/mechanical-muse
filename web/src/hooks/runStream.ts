/**
 * Pure state for one run's event stream. Ticks go to a bounded buffer; everything else
 * (phases, the start receipt, KPIs, the end, any supervisor event) goes to a log that is never
 * trimmed, so a degraded or failed step is never dropped from view.
 */
import type { Envelope } from "@/types/generated/Envelope";
import type { Phase } from "@/types/generated/Phase";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunState } from "@/types/generated/RunState";
import type { SequenceInfo } from "@/types/generated/SequenceInfo";
import type { SupervisorEvent } from "@/types/generated/SupervisorEvent";

type WireValue = number | boolean | string | { class: number; ordinal: number };

export interface TickPoint {
  seq: number;
  /** BOPTEST simulation seconds; null when the readings carried no time. */
  t: number | null;
  outputs: Record<string, Record<string, WireValue>>;
  writes: Record<string, number>;
  readings: Record<string, number>;
}

export interface LogEntry {
  seq: number;
  t: number | null;
  text: string;
  severity: "info" | "warning" | "error";
}

export interface RunStreamState {
  runId: string | null;
  phase: Phase | null;
  config: RunConfig | null;
  sequences: SequenceInfo[];
  ticks: TickPoint[];
  kpi: Record<string, number> | null;
  ended: { state: RunState; message: string | null } | null;
  log: LogEntry[];
  lastSeq: number | null;
  /** True once an event arrived out of order: the view should refetch the run. */
  gap: boolean;
  /** Why the latest step wrote no new setpoints (fail-safe active), or null when it did. */
  degraded: string | null;
}

export const MAX_TICKS = 6000;

export const initialRunStream: RunStreamState = {
  runId: null,
  phase: null,
  config: null,
  sequences: [],
  ticks: [],
  kpi: null,
  ended: null,
  log: [],
  lastSeq: null,
  gap: false,
  degraded: null,
};

function describe(e: SupervisorEvent): LogEntry["text"] {
  const where = [e.sequence, e.point].filter(Boolean).join(" ");
  switch (e.kind) {
    case "clamped":
      return `${where}: clamped ${e.value ?? "?"} to ${e.sent ?? "?"}`;
    case "degraded":
      return `${where}: no new setpoints (${e.reason ?? "unknown reason"})`;
    case "held":
      return `${where}: holding last good value ${e.value ?? "?"}`;
    case "released":
      return `${where}: handed back to the baseline controller`;
    case "stale":
      return "readings did not move forward in time; nothing ran";
    case "injected":
      return `${where}: test fault injected (reading replaced with NaN)`;
    default:
      return `${where}: ${e.kind}`;
  }
}

function severity(e: SupervisorEvent): LogEntry["severity"] {
  return e.kind === "clamped" ? "info" : e.kind === "stale" || e.kind === "degraded" ? "error" : "warning";
}

/** Events meaning a step did not write fresh setpoints. Clamping still writes, so it is not. */
const FAIL_SAFE = new Set(["degraded", "held", "released", "stale", "rejected"]);

export function reduceRun(state: RunStreamState, envs: Envelope[]): RunStreamState {
  let next = state;
  // One copy of the tick buffer per batch, not per event.
  let ticks: TickPoint[] | null = null;
  for (const env of envs) {
    if (next.lastSeq !== null && env.seq <= next.lastSeq) continue;
    const gap = next.gap || (next.lastSeq === null ? env.seq !== 0 : env.seq !== next.lastSeq + 1);
    const t = env.sim_time_s ?? null;
    const base = { ...next, runId: env.run_id, lastSeq: env.seq, gap };
    switch (env.kind) {
      case "phase":
        next = {
          ...base,
          phase: env.phase,
          log: [...base.log, { seq: env.seq, t, text: `phase: ${env.phase}`, severity: "info" }],
        };
        break;
      case "started":
        next = {
          ...base,
          config: env.config,
          sequences: env.sequences,
          log: [
            ...base.log,
            {
              seq: env.seq,
              t,
              text: env.config.baseline
                ? "started: baseline (no writes)"
                : `started: ${env.sequences.length} verified sequences`,
              severity: "info",
            },
          ],
        };
        break;
      case "tick": {
        const point: TickPoint = {
          seq: env.seq,
          t,
          outputs: env.outputs,
          writes: pruneUndefined(env.writes),
          readings: pruneUndefined(env.readings),
        };
        ticks ??= [...base.ticks];
        ticks.push(point);
        const events = env.events.map((e) => ({ seq: env.seq, t, text: describe(e), severity: severity(e) }));
        const failSafe = env.events.find((e) => FAIL_SAFE.has(e.kind));
        next = {
          ...base,
          ticks,
          log: events.length ? [...base.log, ...events] : base.log,
          degraded: failSafe ? describe(failSafe) : null,
        };
        break;
      }
      case "kpi":
        next = { ...base, kpi: pruneUndefined(env.values) };
        break;
      case "ended":
        next = {
          ...base,
          ended: { state: env.state, message: env.message ?? null },
          log: [
            ...base.log,
            {
              seq: env.seq,
              t,
              text: `ended: ${env.state}${env.message ? ` (${env.message})` : ""}`,
              severity: env.state === "failed" ? "error" : "info",
            },
          ],
        };
        break;
      default: {
        // A kind this build does not know: keep going rather than crash.
        const unknown: never = env;
        void unknown;
        next = base;
      }
    }
  }
  if (ticks !== null && ticks.length > MAX_TICKS) {
    ticks.splice(0, ticks.length - MAX_TICKS);
  }
  return next;
}

/**
 * A non-finite reading or write arrives as JSON null (serde writes NaN that way), which the
 * generated map type allows. Dropping it leaves the key absent, and an absent key is shown as
 * "no data", never as 0.
 */
function pruneUndefined(m: { [key in string]?: number }): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [k, v] of Object.entries(m)) if (typeof v === "number") out[k] = v;
  return out;
}
