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

  it("marks a step degraded while the fail-safe acts, and clears it on a clean step", () => {
    const tick = (seq: number, events: SupervisorEvent[]): Envelope => ({
      run_id: "r", seq, sim_time_s: seq * 120, kind: "tick", outputs: {}, writes: {}, readings: {}, events,
    });
    const held = reduceRun(initialRunStream, [
      tick(0, [{ kind: "clamped", point: "p", value: 300, sent: 288.15 }]),
    ]);
    expect(held.degraded).toBeNull();
    const bad = reduceRun(held, [tick(1, [{ kind: "degraded", sequence: "chw", reason: "reading x is missing" }])]);
    expect(bad.degraded).toContain("missing");
    expect(reduceRun(bad, [tick(2, [])]).degraded).toBeNull();
  });
});

describe("injected faults", () => {
  it("logs an injected fault and shows the step as degraded", () => {
    const tick = {
      run_id: "r",
      seq: 0,
      sim_time_s: 100,
      kind: "tick" as const,
      outputs: {},
      writes: {},
      readings: {},
      events: [
        { kind: "injected", point: "floor1_reaAHU_TSup_y", reason: "test fault: reading replaced with NaN" },
        { kind: "degraded", sequence: "ahu1", reason: "floor1_reaAHU_TSup_y is not finite" },
      ],
    };
    const s = reduceRun(initialRunStream, [tick]);
    expect(s.log.map((l) => l.text)).toContain("floor1_reaAHU_TSup_y: test fault injected (reading replaced with NaN)");
    expect(s.degraded).toContain("ahu1");
  });

  it("describes a bias fault as a bias, not as NaN", () => {
    // The event shape of fixtures/replays/day185-noon-fault-sensor-bias.jsonl.
    const tick = {
      run_id: "run-1793150217-3",
      seq: 36,
      sim_time_s: 16030920,
      kind: "tick" as const,
      outputs: {},
      writes: {},
      readings: {},
      events: [
        {
          kind: "injected",
          point: "floor1_reaAHU_TSup_y",
          value: 290.58717267587093,
          reason: "test fault: reading biased by +4",
        },
      ],
    };
    const s = reduceRun({ ...initialRunStream, lastSeq: 35 }, [tick]);
    const texts = s.log.map((l) => l.text);
    expect(texts).toContain("floor1_reaAHU_TSup_y: test fault injected (reading biased by +4)");
    expect(texts.join(" ")).not.toContain("NaN");
  });

  it("falls back to the NaN wording for records without a reason", () => {
    const tick = {
      run_id: "r",
      seq: 0,
      sim_time_s: 100,
      kind: "tick" as const,
      outputs: {},
      writes: {},
      readings: {},
      events: [{ kind: "injected", point: "p" }],
    };
    const s = reduceRun(initialRunStream, [tick]);
    expect(s.log.map((l) => l.text)).toContain("p: test fault injected (reading replaced with NaN)");
  });
});

describe("fail-safe spans", () => {
  const tick = (seq: number, events: SupervisorEvent[]): Envelope => ({
    run_id: "r",
    seq,
    sim_time_s: 100 + seq,
    kind: "tick",
    outputs: {},
    writes: {},
    readings: {},
    events,
  });
  it("keeps the steps the fail-safe was active after it ends", () => {
    const degraded: SupervisorEvent = { kind: "degraded", sequence: "ahu1", reason: "not finite" };
    const envs = [0, 1, 2, 3, 4, 5, 6].map((i) => tick(i, i === 2 || i === 3 || i === 5 ? [degraded] : []));
    const s = reduceRun(initialRunStream, envs);
    expect(s.degraded).toBeNull();
    expect(s.steps).toBe(7);
    expect(s.failSafeSpans).toEqual([
      { from: 2, to: 3 },
      { from: 5, to: 5 },
    ]);
  });
  it("does not count a clamp as fail-safe", () => {
    const s = reduceRun(initialRunStream, [tick(0, [{ kind: "clamped", point: "p", value: 1, sent: 0 }])]);
    expect(s.failSafeSpans).toEqual([]);
  });
});
