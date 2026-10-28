import { describe, expect, it } from "vitest";

import type { SequenceDraft } from "@/types/generated/SequenceDraft";

import {
  MAX_TOTAL_BYTES,
  acceptedDraft,
  clock,
  revisionDraft,
  revisionName,
  totalProblem,
  withExpected,
} from "./workbench";

const draft: SequenceDraft = {
  name: "fan",
  title: "Fan",
  description: "",
  inputs: [],
  outputs: [],
  params: [],
  step_s: 60,
  horizon_s: 600,
  open_questions: [],
  scenarios: [
    { name: "a", description: "", rationale: "", inputs: [], expect: [{ output: "y", from_s: 0, to_s: 60, equals: 1 }] },
    { name: "b", description: "", rationale: "", inputs: [], expect: [{ output: "y", from_s: 0, to_s: 60, equals: 2 }] },
  ],
};

describe("workbench helpers", () => {
  it("edits one expected value in one scenario only", () => {
    const edited = withExpected(draft, "b", 0, 5);
    expect(edited.scenarios[1]?.expect[0]?.equals).toBe(5);
    expect(edited.scenarios[0]?.expect[0]?.equals).toBe(1);
    expect(draft.scenarios[1]?.expect[0]?.equals).toBe(2);
  });

  it("accepts only the kept scenarios", () => {
    expect(acceptedDraft(draft, new Set(["b"])).scenarios.map((s) => s.name)).toEqual(["b"]);
  });

  it("keeps attachments under the server's request limit", () => {
    const big = { name: "a.pdf", mime: "application/pdf", data_base64: "A".repeat(12 * 1024 * 1024) };
    expect(totalProblem([], MAX_TOTAL_BYTES)).toBeNull();
    expect(totalProblem([big], 6 * 1024 * 1024)).toMatch(/14 MB/);
  });

  it("formats scenario times", () => {
    expect(clock(90)).toBe("1:30");
    expect(clock(3725)).toBe("1:02:05");
  });
});

describe("revisions", () => {
  it("names the next free revision", () => {
    expect(revisionName("fan", [])).toBe("fan_r2");
    expect(revisionName("fan", ["fan_r2"])).toBe("fan_r3");
    expect(revisionName("fan_r2", [])).toBe("fan_r3");
    expect(revisionName("fan_r9", ["fan_r10"])).toBe("fan_r11");
  });

  it("adds the chosen proposals after the accepted scenarios", () => {
    const edge = { name: "edge", description: "", rationale: "", inputs: [], expect: [] };
    const r = revisionDraft(draft, [edge], "fan_r2");
    expect(r.name).toBe("fan_r2");
    expect(r.scenarios.map((s) => s.name)).toEqual(["a", "b", "edge"]);
    expect(draft.name).toBe("fan");
    expect(draft.scenarios).toHaveLength(2);
  });
});
