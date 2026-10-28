import { describe, expect, it } from "vitest";

import type { DraftInfo } from "@/types/generated/DraftInfo";

import { draftsOf, newestDraft } from "./drafts";

function draft(content_id: string, spec: string, stored_seq: number): DraftInfo {
  return { content_id, spec, stored_seq, scenarios: 10, model: "muse-spark-1.3", source: "live", session: "s", attempts_used: 1 };
}

describe("drafts", () => {
  // Content ids sort the opposite way to store order here, as hashes may.
  const list = [draft("cxf:a", "x", 1), draft("cxf:z", "x", 3), draft("cxf:m", "y", 4), draft("cxf:b", "x", 2), draft("cxf:old", "x", 0)];

  it("lists one spec's drafts newest first, older unordered ones last", () => {
    expect(draftsOf(list, "x").map((d) => d.content_id)).toEqual(["cxf:z", "cxf:b", "cxf:a", "cxf:old"]);
  });

  it("picks the newest draft of a spec", () => {
    expect(newestDraft(list, "x")?.content_id).toBe("cxf:z");
    expect(newestDraft(list, "none")).toBeNull();
  });
});
