import { readFileSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import type { Envelope } from "@/types/generated/Envelope";
import type { SupervisorEvent } from "@/types/generated/SupervisorEvent";

import { initialRunStream, MAX_TICKS, reduceRun } from "./runStream";

/** Events recorded from a live run (trimmed: seq 0-7, then the KPI and end, 35-36). */
const recorded: Envelope[] = readFileSync(
  path.resolve(import.meta.dirname, "../../../fixtures/runs/live-chw-requests.jsonl"),
  "utf8",
)
  .trim()
  .split("\n")
  .map((line) => JSON.parse(line) as Envelope);

describe("reduceRun", () => {
  it("applies a recorded live run", () => {
    const s = reduceRun(initialRunStream, recorded);
    expect(s.runId).toBe(recorded[0]?.run_id);
    expect(s.phase).toBe("running");
    expect(s.config?.requests).toBe(true);
    expect(s.sequences.map((q) => q.name)).toEqual(["ahu1", "ahu2", "ahu3", "chw"]);
    expect(s.ticks).toHaveLength(3);
    expect(s.ticks[0]?.t).toBe(16027200);
    expect(s.ticks[0]?.writes["oveChiWatSys_TWSet_u"]).toBe(278.71);
    expect(s.ticks[0]?.outputs["ahu1"]?.["yChiWatResReq"]).toBe(0);
    expect(s.kpi?.["ener_tot"]).toBeGreaterThan(0);
    expect(s.ended).toEqual({ state: "completed", message: null });
    // The trimmed recording skips 8-34: the reducer notices.
    expect(s.gap).toBe(true);
  });

  it("ignores repeats and keeps order", () => {
    const first = recorded.slice(0, 6);
    const once = reduceRun(initialRunStream, first);
    const twice = reduceRun(once, first);
    expect(twice).toBe(once);
    expect(once.gap).toBe(false);
    expect(once.lastSeq).toBe(5);
  });

  it("flags a stream that does not start at 0", () => {
    expect(reduceRun(initialRunStream, recorded.slice(2, 4)).gap).toBe(true);
  });

  it("bounds ticks but never drops supervisor events", () => {
    const tick = (seq: number, events: SupervisorEvent[]): Envelope => ({
      run_id: "r",
      seq,
      sim_time_s: seq * 120,
      kind: "tick",
      outputs: {},
      writes: {},
      readings: {},
      events,
    });
    const envs: Envelope[] = [];
    for (let i = 0; i < MAX_TICKS + 50; i++) {
      envs.push(tick(i, i === 1 ? [{ kind: "degraded", sequence: "chw", reason: "reading x is not finite" }] : []));
    }
    const s = reduceRun(initialRunStream, envs);
    expect(s.ticks).toHaveLength(MAX_TICKS);
    expect(s.ticks[0]?.seq).toBe(50);
    expect(s.log).toHaveLength(1);
    expect(s.log[0]?.severity).toBe("error");
    expect(s.log[0]?.text).toContain("not finite");
  });

  it("marks a failed run as an error", () => {
    const s = reduceRun(initialRunStream, [
      { run_id: "r", seq: 0, kind: "ended", state: "failed", message: "initializing BOPTEST: timeout" },
    ]);
    expect(s.log.at(-1)?.severity).toBe("error");
    expect(s.ended?.message).toContain("timeout");
  });
});
