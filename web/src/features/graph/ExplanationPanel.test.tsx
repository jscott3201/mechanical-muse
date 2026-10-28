import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ExplanationView } from "@/types/generated/ExplanationView";

import { ExplanationPanel } from "./ExplanationPanel";

const recorded: ExplanationView = {
  spec: "chw_plant_reset",
  content_id: "cxf:fnv1a128:d61c",
  model: "muse-spark-1.3",
  source: "recorded",
  summary: "Relaxes the plant while the air handlers are satisfied.",
  parts: [
    { title: "Enable and update timing", text: "Waits after enable.", blocks: ["startDelay", "sampleTick"] },
    { title: "Map reset to setpoints", text: "Two linear maps.", blocks: ["tempMap", "dpMap"] },
  ],
  unknown_blocks: ["ghost"],
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("ExplanationPanel", () => {
  it("asks for an explanation, labels its source and outlines a part's blocks", async () => {
    const fetch = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(recorded), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    const focus = vi.fn();
    render(<ExplanationPanel spec="chw_plant_reset" onFocusBlocks={focus} />);
    fireEvent.click(screen.getByRole("button", { name: /Explain with Muse Spark/ }));
    expect(await screen.findByText(/Written by muse-spark-1.3/)).toBeTruthy();
    expect(screen.getByText(/\(recorded\)/)).toBeTruthy();
    expect(fetch.mock.calls[0]?.[0]).toBe("/api/specs/chw_plant_reset/explain");
    fireEvent.mouseEnter(screen.getByRole("button", { name: /Map reset to setpoints/ }));
    expect(focus).toHaveBeenLastCalledWith(["tempMap", "dpMap"]);
    // Blocks the model named that the graph lacks are reported, not shown as parts.
    expect(screen.getByText(/1 block this graph does not have \(\s*ghost\)/)).toBeTruthy();
  });

  it("offers no retry on a keyless server and says a model key is needed", async () => {
    const body = { code: "no-model", message: "no model key on this server and no recorded explanation of this graph" };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status: 503 })));
    render(<ExplanationPanel spec="chw_plant_reset" onFocusBlocks={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Explain with Muse Spark/ }));
    expect(await screen.findByText(/needs a model key on the server/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });
});
