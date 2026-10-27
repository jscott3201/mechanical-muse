import { describe, expect, it } from "vitest";

import { journey } from "./journey";

const states = (steps: ReturnType<typeof journey>) => steps.map((s) => `${s.id}:${s.state}`);

describe("journey", () => {
  it("starts at reading the document", () => {
    const steps = journey({ read: false, spec: null, verified: null, bound: false });
    expect(states(steps)).toEqual(["read:next", "accept:todo", "draft:todo", "bind:todo", "run:todo"]);
    // Steps that need a sequence cannot be reached before one exists.
    expect(steps.filter((s) => s.to).map((s) => s.id)).toEqual(["read", "accept"]);
  });

  it("marks each step from data and links the run to the newest verified draft", () => {
    expect(states(journey({ read: false, spec: "sat", verified: null, bound: true }))).toEqual([
      "read:done",
      "accept:done",
      "draft:next",
      "bind:done",
      "run:todo",
    ]);
    const ready = journey({ read: false, spec: "sat", verified: "cxf:1", bound: true });
    expect(states(ready).at(-1)).toBe("run:next");
    expect(ready.at(-1)?.to).toEqual({ page: "runs", draft: { spec: "sat", contentId: "cxf:1" } });
  });
});
