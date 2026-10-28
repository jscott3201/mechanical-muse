import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { RunExplanationView } from "@/types/generated/RunExplanationView";

import { RunExplanationPanel } from "./RunExplanationPanel";

const recorded: RunExplanationView = {
  key: "run:b516292f3857089b",
  model: "muse-spark-1.3",
  source: "recorded",
  summary: "A planned sensor fault paused the sequences, which then resumed.",
  items: [{ when: "day 186 00:40 to day 186 00:48", title: "Planned sensor fault test", text: "A test replaced a reading." }],
  spans: 5,
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("RunExplanationPanel", () => {
  it("asks for the run's explanation and labels it as model-written and recorded", async () => {
    const fetch = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(recorded), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    render(<RunExplanationPanel runId="run-7" ended />);
    fireEvent.click(screen.getByRole("button", { name: /Explain these events/ }));
    expect(await screen.findByText(/Written by muse-spark-1.3/)).toBeTruthy();
    expect(screen.getByText(/\(recorded\)/)).toBeTruthy();
    expect(screen.getByText("Planned sensor fault test")).toBeTruthy();
    expect(fetch.mock.calls[0]?.[0]).toBe("/api/runs/run-7/explain");
  });

  it("offers no retry on a keyless server and says a model key is needed", async () => {
    const body = { code: "no-model", message: "no model key on this server and no recorded explanation of this run" };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status: 503 })));
    render(<RunExplanationPanel runId="run-7" ended />);
    fireEvent.click(screen.getByRole("button", { name: /Explain these events/ }));
    expect(await screen.findByText(/needs a model key on the server/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("still offers a retry for other failures", async () => {
    const body = { code: "model-error", message: "the model call failed" };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status: 502 })));
    render(<RunExplanationPanel runId="run-7" ended />);
    fireEvent.click(screen.getByRole("button", { name: /Explain these events/ }));
    expect(await screen.findByRole("button", { name: "Try again" })).toBeTruthy();
  });
});
