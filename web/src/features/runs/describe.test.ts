import { describe as suite, expect, it } from "vitest";

import type { ReplayInfo } from "@/types/generated/ReplayInfo";

import { byWindow, configFacts, describe, replayFacts, windowLabel } from "./describe";

const replay = (over: Partial<ReplayInfo>): ReplayInfo => ({
  name: "r",
  recorded_run: "run-1",
  ticks: 300,
  baseline: false,
  case: "multizone_office_complex_air",
  step_s: 120,
  start_s: 16027200,
  drafted: false,
  drafts: [],
  pressure: false,
  own: [],
  requests: false,
  draft_ids: {},
  ...over,
});

suite("describe", () => {
  it("labels an engineer's sequence as theirs, with a hand-built plant reset", () => {
    // day185-noon-your-sequence: drafted is true only because of the engineer's own draft.
    const r = replay({ drafted: true, drafts: ["ahu1_sat_stpt"], own: ["ahu1_sat_stpt"] });
    const text = describe(replayFacts(r));
    expect(text).toBe("Your sequence ahu1_sat_stpt (Spark draft); plant reset hand-built");
    expect(text).not.toContain("Spark-drafted plant reset");
  });

  it("says when the plant reset beside an engineer's sequence was drafted too", () => {
    const r = replay({ drafted: true, drafts: ["ahu1_sat_stpt", "chw_plant_reset"], own: ["ahu1_sat_stpt"], pressure: true });
    expect(describe(replayFacts(r))).toBe("Your sequence ahu1_sat_stpt (Spark draft); plant reset Spark-drafted, with static pressure");
  });

  it("keeps the reference labels", () => {
    expect(describe(replayFacts(replay({ drafted: true, drafts: ["chw_plant_reset"] })))).toBe("Spark-drafted plant reset, chilled water");
    expect(describe(replayFacts(replay({ pressure: true })))).toBe("Hand-built sequences, chilled water and static pressure");
    expect(describe(replayFacts(replay({ baseline: true })))).toBe("BOPTEST baseline, nothing written");
  });

  it("reads a run config, tolerating old records without drafts or bindings", () => {
    const cfg = { baseline: false, requests: true, pressure: false, start_s: 0, warmup_s: 0, steps: 1, step_s: 120, case: "c" };
    // An old record really can lack these fields; the type says they are always there.
    expect(describe(configFacts(cfg as never))).toBe("Hand-built sequences, chilled water");
    const own = { ...cfg, drafts: { sat: "cxf:1" }, bindings: { sat: { floors: [1], inputs: [], outputs: [], notes: [], units: {} } } };
    expect(describe(configFacts(own))).toBe("Your sequence sat (Spark draft); plant reset hand-built");
  });
});

suite("windows", () => {
  it("names the usual windows and groups replays by start", () => {
    expect(windowLabel(12376800)).toBe("Day 143 morning");
    expect(windowLabel(16027200)).toBe("Day 185 noon");
    expect(windowLabel(16070400)).toBe("Day 186 midnight");
    expect(windowLabel(16027200 + 3 * 3600 + 1800)).toBe("Day 185 15:30");
    const groups = byWindow([replay({ name: "b", start_s: 16027200 }), replay({ name: "a", start_s: 12376800 }), replay({ name: "c", start_s: 16027200 })]);
    expect(groups.map((g) => [g.label, g.items.map((i) => i.name)])).toEqual([
      ["Day 143 morning", ["a"]],
      ["Day 185 noon", ["b", "c"]],
    ]);
  });
});

suite("faults", () => {
  it("names a test fault, so a faulted run never reads like its healthy twin", () => {
    const healthy = replay({ drafted: false, drafts: [], own: [] });
    const faulted = replay({
      drafted: false,
      drafts: [],
      own: [],
      fault: { point: "floor1_reaAHU_TSup_y", from_step: 30, steps: 60, mode: { kind: "bias", offset: 4 } },
    });
    expect(describe(replayFacts(healthy))).toBe("Hand-built sequences, chilled water");
    expect(describe(replayFacts(faulted))).toBe("Hand-built sequences, chilled water; test fault: floor1_reaAHU_TSup_y biased by +4 K");
  });
});
