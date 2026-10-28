import { describe, expect, it } from "vitest";

import { NO_RECORDING, NO_TOOLS_RECORDING, ONLY_TOOLS_RECORDING, recordedBlocks } from "./recorded";

describe("recordedBlocks", () => {
  it("blocks nothing for a live call or while the answer is unknown", () => {
    expect(recordedBlocks({ plain: false, tools: false }, "live", true)).toEqual({ draft: null, tools: null });
    expect(recordedBlocks(null, "recorded", false)).toEqual({ draft: null, tools: null });
  });

  it("blocks Draft and the tools switch on a spec with no recording", () => {
    expect(recordedBlocks({ plain: false, tools: false }, "recorded", false)).toEqual({
      draft: NO_RECORDING,
      tools: NO_TOOLS_RECORDING,
    });
    // A switch left on from a live call does not hide the reason.
    expect(recordedBlocks({ plain: false, tools: false }, "recorded", true).draft).toBe(NO_RECORDING);
  });

  it("falls back to the plain recording when there is none with tools", () => {
    expect(recordedBlocks({ plain: true, tools: false }, "recorded", true)).toEqual({ draft: null, tools: NO_TOOLS_RECORDING });
  });

  it("points at the tools switch when only a tools recording exists", () => {
    expect(recordedBlocks({ plain: false, tools: true }, "recorded", false)).toEqual({ draft: ONLY_TOOLS_RECORDING, tools: null });
    expect(recordedBlocks({ plain: false, tools: true }, "recorded", true)).toEqual({ draft: null, tools: null });
  });
});
