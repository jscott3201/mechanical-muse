import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AttackView } from "@/types/generated/AttackView";
import type { ScenarioDraft } from "@/types/generated/ScenarioDraft";

import { AttackPanel } from "./AttackPanel";

function scenario(name: string, rationale: string): ScenarioDraft {
  return {
    name,
    description: "",
    rationale,
    inputs: [{ point: "u", steps: [{ t: 0, value: 1 }] }],
    expect: [{ output: "y", from_s: 0, to_s: 60, equals: true }],
  };
}

const view: AttackView = {
  key: "attack:00ff",
  model: "muse-spark-1.3",
  source: "recorded",
  draft: "cxf:fnv1a128:abc",
  sequence: {
    name: "sat",
    title: "SAT",
    description: "",
    inputs: [{ name: "u", type: "real", unit: "K", meaning: "" }],
    outputs: [{ name: "y", type: "bool", unit: "bool", meaning: "" }],
    params: [],
    step_s: 60,
    horizon_s: 600,
    open_questions: [],
    scenarios: [scenario("accepted_one", "")],
  },
  proposals: [
    { scenario: scenario("exact_threshold", "Probes the value exactly at the threshold."), result: "passed" },
    {
      scenario: scenario("hold_ends", "Probes the last tick of the hold."),
      result: "failed",
      failures: [
        { name: "hold_ends", ticks: 3, result: "fail", t: 120, output: "y", expected: true, actual: false, from_s: 0, to_s: 60 },
      ],
    },
  ],
  problems: ["dropped `off_grid`: off the clock"],
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AttackPanel", () => {
  it("shows labelled proposals with pass and fail in words, and accepts the chosen ones as a revision", async () => {
    const calls: { url: string; method: string; body: unknown }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        calls.push({ url, method: init?.method ?? "GET", body });
        if (url.endsWith("/api/workbench/sat/attack")) return new Response(JSON.stringify(view), { status: 200 });
        if (url.endsWith("/api/workbench/accept"))
          return new Response(
            JSON.stringify({ spec: { summary: { name: body.draft.name, scenarios: body.draft.scenarios.length } } }),
            { status: 201 },
          );
        if (url.endsWith("/api/workbench")) return new Response(JSON.stringify([{ name: "sat" }, { name: "sat_r2" }]), { status: 200 });
        return new Response('{"code":"not-found","message":"no"}', { status: 404 });
      }),
    );
    render(<AttackPanel spec="sat" draft="cxf:fnv1a128:abc" />);
    fireEvent.click(screen.getByRole("button", { name: /Try to break it/ }));
    expect(await screen.findByText(/Proposed by Muse Spark \(recorded\) — tests, not verdicts/)).toBeTruthy();
    expect(calls[0]).toMatchObject({ method: "POST", body: { draft: "cxf:fnv1a128:abc" } });
    expect(screen.getByText("Passes this draft")).toBeTruthy();
    expect(screen.getByText("Fails this draft")).toBeTruthy();
    expect(screen.getByText(/1 of 2 proposals pass this draft/)).toBeTruthy();
    expect(screen.getByText(/the draft or the proposal is wrong/)).toBeTruthy();
    expect(
      screen.getByText((_, el) => el?.tagName === "SPAN" && el.textContent === "y at 120 s: expected true, got false (window 0–60 s)"),
    ).toBeTruthy();
    expect(screen.getByText(/off the clock/)).toBeTruthy();

    // Nothing is chosen until the engineer chooses; the next free name is suggested.
    const accept = await screen.findByRole("button", { name: /Accept selected as revision sat_r3/ });
    expect((accept as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: "Choose hold_ends" }));
    expect((accept as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(accept);
    expect(await screen.findByText("Accepted as sat_r3")).toBeTruthy();
    const sent = calls.find((c) => c.url.endsWith("/api/workbench/accept"));
    expect(sent?.body).toMatchObject({
      key: "attack:00ff",
      draft: { name: "sat_r3", title: "SAT", scenarios: [{ name: "accepted_one" }, { name: "hold_ends" }] },
    });
  });

  it("says what is missing on a keyless server instead of offering a retry", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response('{"code":"no-model","message":"no model key"}', { status: 503 })),
    );
    render(<AttackPanel spec="sat" draft="cxf:x" />);
    fireEvent.click(screen.getByRole("button", { name: /Try to break it/ }));
    expect(await screen.findByText("no-model")).toBeTruthy();
    expect(screen.getByText(/Proposals need a model key/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });
});
