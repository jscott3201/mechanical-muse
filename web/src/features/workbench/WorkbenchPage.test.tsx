import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ReadView } from "@/types/generated/ReadView";

import { WorkbenchPage } from "./WorkbenchPage";

const view: ReadView = {
  key: "read:abc",
  model: "muse-spark-1.3",
  source: "recorded",
  problems: [],
  draft: {
    name: "fan_on_hot",
    title: "Exhaust fan on high temperature",
    description: "Run the fan when the room is hot.",
    inputs: [{ name: "TRoo", type: "real", unit: "K", meaning: "Room temperature" }],
    outputs: [{ name: "y1Fan", type: "bool", unit: "1", meaning: "Fan command" }],
    params: [{ name: "TOn", value: 300.15, unit: "K", meaning: "On above" }],
    step_s: 60,
    horizon_s: 1200,
    open_questions: ["Is the comparison strict?"],
    scenarios: [
      {
        name: "hot",
        description: "A hot room turns the fan on.",
        rationale: "pins the threshold",
        inputs: [{ point: "TRoo", steps: [{ t: 0, value: 295.15 }, { t: 600, value: 303.15 }] }],
        expect: [{ output: "y1Fan", from_s: 660, to_s: 1200, equals: true }],
      },
      {
        name: "cool",
        description: "A cool room keeps it off.",
        rationale: "pins the off state",
        inputs: [{ point: "TRoo", steps: [{ t: 0, value: 290 }] }],
        expect: [{ output: "y1Fan", from_s: 0, to_s: 1200, equals: false }],
      },
    ],
  },
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("WorkbenchPage", () => {
  it("reads pasted text, shows the labelled proposal, and accepts only the kept scenarios", async () => {
    const calls: { url: string; body: unknown }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        calls.push({ url, body });
        if (url === "/api/workbench/read") return new Response(JSON.stringify(view), { status: 200 });
        if (url === "/api/workbench/accept")
          return new Response(JSON.stringify({ spec: { summary: { name: "fan_on_hot" } } }), { status: 201 });
        return new Response('{"code":"not-found","message":"no"}', { status: 404 });
      }),
    );
    const opened = vi.fn();
    render(<WorkbenchPage onOpenSpec={opened} />);
    // The journey bar marks the step in words, not only by colour.
    expect(screen.getByRole("link", { name: "Read (next)" })).toBeTruthy();
    expect(screen.getByText("Draft & verify")).toBeTruthy();
    const read = screen.getByRole("button", { name: /Read with Muse Spark/ });
    expect((read as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Sequence text"), { target: { value: "Run the fan above 27 °C." } });
    fireEvent.click(read);

    expect(await screen.findByText(/Read by muse-spark-1.3/)).toBeTruthy();
    expect(screen.getByText(/\(recorded\)/)).toBeTruthy();
    expect(screen.getByText("Is the comparison strict?")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Read (done)" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Accept (next)" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Timeline of scenario hot" })).toBeTruthy();
    expect(calls[0]?.body).toEqual({ text: "Run the fan above 27 °C.", attachments: [] });

    fireEvent.click(screen.getByRole("switch", { name: "Keep cool" }));
    fireEvent.click(screen.getByRole("button", { name: /Accept 1 scenario and create the sequence/ }));
    await vi.waitFor(() => expect(opened).toHaveBeenCalledWith("fan_on_hot"));
    const accept = calls.find((c) => c.url === "/api/workbench/accept")?.body as {
      key: string;
      draft: { scenarios: { name: string }[] };
    };
    expect(accept.key).toBe("read:abc");
    expect(accept.draft.scenarios.map((s) => s.name)).toEqual(["hot"]);
  });
});
