import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { KINDS } from "./useRunStream";

// An EventSource drops event names nobody listens for, and the view then reports a gap.
describe("useRunStream kinds", () => {
  it("listens for every kind the committed replays contain", () => {
    const dir = join(__dirname, "../../../fixtures/replays");
    const seen = new Set<string>();
    for (const name of readdirSync(dir).filter((n) => n.endsWith(".jsonl"))) {
      for (const line of readFileSync(join(dir, name), "utf8").split("\n")) {
        if (line.trim()) seen.add((JSON.parse(line) as { kind: string }).kind);
      }
    }
    expect(seen.has("peak-power")).toBe(true);
    for (const kind of seen) expect(KINDS).toContain(kind);
  });
});
