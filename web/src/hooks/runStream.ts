/**
 * Pure state for one run's event stream. Ticks go to a bounded buffer; everything else
 * (phases, the start receipt, KPIs, the end, any supervisor event) goes to a log that is never
 * trimmed, so a degraded or failed step is never dropped from view.
 */
import { formatNumber } from "@/lib/format";
import type { Envelope } from "@/types/generated/Envelope";
import type { Phase } from "@/types/generated/Phase";
import type { PeakPower } from "@/types/generated/PeakPower";
import type { RunConfig } from "@/types/generated/RunConfig";
import type { RunState } from "@/types/generated/RunState";
import type { SequenceInfo } from "@/types/generated/SequenceInfo";
import type { SupervisorEvent } from "@/types/generated/SupervisorEvent";

/** A tick as the server sends it; its fields' types come from the generated envelope. */
type TickEnvelope = Extract<Envelope, { kind: "tick" }>;

export interface TickPoint {
  seq: number;
  /** BOPTEST simulation seconds; null when the readings carried no time. */
  t: number | null;
  outputs: TickEnvelope["outputs"];
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
  /** A home run's power in its most expensive quarter of prices (ours), at the end. */
  peak: PeakPower | null;
  ended: { state: RunState; message: string | null } | null;
  log: LogEntry[];
  lastSeq: number | null;
  /** True once an event arrived out of order: the view should refetch the run. */
  gap: boolean;
  /** Why the latest step wrote no new setpoints (fail-safe active), or null when it did. */
  degraded: string | null;
  /** Ticks seen so far (the step index of the next tick); never trimmed with the buffer. */
  steps: number;
  /**
   * Runs of steps (0-based, inclusive) where the fail-safe was active, so a short degraded spell
   * stays visible after it ends.
   */
  failSafeSpans: { from: number; to: number }[];
}

export const MAX_TICKS = 6000;

export const initialRunStream: RunStreamState = {
  runId: null,
  phase: null,
  config: null,
  sequences: [],
  ticks: [],
  kpi: null,
  peak: null,
  ended: null,
  log: [],
  lastSeq: null,
  gap: false,
  degraded: null,
  steps: 0,
  failSafeSpans: [],
};

function describe(e: SupervisorEvent): LogEntry["text"] {
  const where = [e.sequence, e.point].filter(Boolean).join(" ");
  switch (e.kind) {
    case "clamped":
      return `${where}: clamped ${e.value === undefined ? "?" : formatNumber(e.value, 3)} to ${e.sent === undefined ? "?" : formatNumber(e.sent, 3)}`;
    case "degraded":
      return `${where}: no new setpoints (${e.reason ?? "unknown reason"})`;
    case "held":
      return `${where}: holding last good value ${e.value ?? "?"}`;
    case "released":
      return `${where}: handed back to the baseline controller`;
    case "stale":
      return "readings did not move forward in time; nothing ran";
    case "injected":
      return `${where}: test fault injected (${injectedWhat(e.reason)})`;
    default:
      return `${where}: ${e.kind}`;
  }
}

/** A log line shown once for a run of identical routine lines. */
export interface LogGroup extends LogEntry {
  count: number;
  /** Simulation time of the last line in the group. */
  lastT: number | null;
}

/**
 * The log for display: identical routine (info) lines, such as the same clamp on every step,
 * are shown once with a count, at their latest occurrence. A warning or error is never merged
 * and ends every open group, so the order of anything that matters is kept.
 */
export function collapseLog(log: readonly LogEntry[]): LogGroup[] {
  const groups: { g: LogGroup; at: number }[] = [];
  let open = new Map<string, { g: LogGroup; at: number }>();
  log.forEach((e, i) => {
    if (e.severity !== "info") {
      open = new Map();
      groups.push({ g: { ...e, count: 1, lastT: e.t }, at: i });
      return;
    }
    const same = open.get(e.text);
    if (same) {
      same.g.count += 1;
      same.g.lastT = e.t;
      same.at = i;
      return;
    }
    const fresh = { g: { ...e, count: 1, lastT: e.t }, at: i };
    open.set(e.text, fresh);
    groups.push(fresh);
  });
  return groups.sort((a, b) => a.at - b.at).map((x) => x.g);
}

/**
 * What an injected fault did to the reading. The server's reason is mode-aware ("test fault:
 * reading biased by +4"); records from before fault modes carry the NaN wording or none.
 */
function injectedWhat(reason: string | undefined): string {
  const what = reason?.replace(/^test fault:\s*/, "").trim();
  return what ? what : "reading replaced with NaN";
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
          steps: base.steps + 1,
          failSafeSpans: failSafe ? extendSpans(base.failSafeSpans, base.steps) : base.failSafeSpans,
        };
        break;
      }
      case "kpi":
        next = { ...base, kpi: pruneUndefined(env.values) };
        break;
      case "peak-power":
        next = { ...base, peak: env.peak };
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

function extendSpans(spans: RunStreamState["failSafeSpans"], step: number): RunStreamState["failSafeSpans"] {
  const last = spans[spans.length - 1];
  if (last && last.to === step - 1) return [...spans.slice(0, -1), { from: last.from, to: step }];
  return [...spans, { from: step, to: step }];
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
