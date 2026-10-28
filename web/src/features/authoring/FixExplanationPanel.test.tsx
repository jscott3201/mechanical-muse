import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { FixExplanationView } from "@/types/generated/FixExplanationView";

import { FixExplanationPanel } from "./FixExplanationPanel";

const recorded: FixExplanationView = {
  key: "fix:919abb74feebf583",
  model: "muse-spark-1.3",
  source: "recorded",
  before: 2,
  after: 3,
  summary: "The earlier draft held the setpoint too long; the later one released `stageHold` at once.",
  changes: [{ title: "Remove false hold on staging", text: "Changed `stageHold` falseHoldDuration to 0.0 s." }],
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("FixExplanationPanel", () => {
  it("asks for the repair between two attempts and labels it as model-written", async () => {
    const fetch = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(recorded), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    render(<FixExplanationPanel sessionId="author-1" after={3} />);
    expect(screen.getByText(/What changed since attempt 2/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Explain the fix/ }));
    expect(await screen.findByText(/Written by muse-spark-1.3/)).toBeTruthy();
    expect(screen.getByText(/\(recorded\)/)).toBeTruthy();
    expect(screen.getByText(/The verdict above is the engine's/)).toBeTruthy();
    expect(screen.getByText("Remove false hold on staging")).toBeTruthy();
    // Backticked block ids render as code, without the backticks.
    expect(screen.getAllByText("stageHold")[0]?.tagName).toBe("CODE");
    expect(fetch.mock.calls[0]?.[0]).toBe("/api/authoring/author-1/attempts/3/explain");
  });
});
