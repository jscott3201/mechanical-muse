import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import type { AuthorEnvelope } from "@/types/generated/AuthorEnvelope";

import { checkText } from "@/features/authoring/checkText";

import { initialAuthorStream, reduceAuthor } from "./authorStream";

/** A recorded plant-reset session as the server streamed it: refused, 8 of 10, then green. */
const envs = readFileSync(
  path.resolve(import.meta.dirname, "../../../fixtures/authoring/chw_plant_reset-recorded.jsonl"),
  "utf8",
)
  .trim()
  .split("\n")
  .map((l) => JSON.parse(l) as AuthorEnvelope);

describe("reduceAuthor", () => {
  it("follows a session from drafting to a verified draft", () => {
    const s = reduceAuthor(initialAuthorStream, envs);
    expect(s.config?.source).toBe("recorded");
    expect(s.attempts.map((a) => a.view?.result)).toEqual(["load-failed", "replay-failed", "passed"]);
    expect(s.attempts.map((a) => a.of)).toEqual([3, 3, 3]);
    expect(s.ended?.state).toBe("verified");
    expect(s.ended?.draft?.content_id).toMatch(/^cxf:/);
    expect(s.gap).toBe(false);
  });

  it("shows an attempt as drafting until it is checked", () => {
    const s = reduceAuthor(initialAuthorStream, envs.slice(0, 4));
    expect(s.attempts.map((a) => [a.index, a.view === null])).toEqual([
      [1, false],
      [2, true],
    ]);
    expect(s.ended).toBeNull();
  });

  it("applies each event once, whatever the batching", () => {
    let s = initialAuthorStream;
    for (const e of envs) s = reduceAuthor(s, [e, e]);
    expect(s).toEqual(reduceAuthor(initialAuthorStream, envs));
  });

  it("notes a gap when an event is missing", () => {
    const s = reduceAuthor(initialAuthorStream, [envs[0], envs[2]].filter((e) => e !== undefined));
    expect(s.gap).toBe(true);
  });

  it("keeps Spark's own checks with their attempt, apart from the receipt", () => {
    const id = "author-1";
    const call = (seq: number, ok: boolean, summary: string): AuthorEnvelope => ({
      session_id: id,
      seq,
      kind: "tool-call",
      attempt: 1,
      call: { name: "replay_scenario", args: "s1, draft with 9 blocks", ok, summary },
    });
    const head = envs.slice(0, 2).map((e, seq) => ({ ...e, session_id: id, seq }));
    const s = reduceAuthor(initialAuthorStream, [
      ...head,
      call(2, false, "scenario s1 failed at 240 s: y expected 285.93, got 291.48"),
      call(3, true, "scenario s1 passed"),
    ]);
    expect(s.attempts).toHaveLength(1);
    const entry = s.attempts[0];
    expect(entry?.view).toBeNull();
    expect(entry?.checks.map((c) => c.ok)).toEqual([false, true]);
    expect(entry?.checks.map(checkText)).toEqual([
      "Spark replayed scenario s1: failed at 240 s, y expected 285.93, got 291.48",
      "Spark replayed scenario s1: passed",
    ]);
    // Logs recorded before tools existed have no tool_calls on their attempts.
    const old = reduceAuthor(initialAuthorStream, envs);
    expect(old.attempts.every((a) => a.checks.length === 0)).toBe(true);
  });
});
