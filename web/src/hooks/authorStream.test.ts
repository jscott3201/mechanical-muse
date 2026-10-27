import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import type { AuthorEnvelope } from "@/types/generated/AuthorEnvelope";

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
});
