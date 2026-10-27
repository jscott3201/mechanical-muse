import { act, cleanup, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { RunsPage } from "./RunsPage";

const lines = readFileSync(
  path.resolve(import.meta.dirname, "../../../../fixtures/runs/live-chw-requests.jsonl"),
  "utf8",
)
  .trim()
  .split("\n")
  // The recording is trimmed (it skips 8-34); a real stream is contiguous, so renumber.
  .map((line, i) => JSON.stringify({ ...(JSON.parse(line) as object), seq: i }));
const gapped = readFileSync(
  path.resolve(import.meta.dirname, "../../../../fixtures/runs/live-chw-requests.jsonl"),
  "utf8",
)
  .trim()
  .split("\n");
const runId = (JSON.parse(lines[0] ?? "{}") as { run_id: string }).run_id;

class FakeSource {
  static last: FakeSource | null = null;
  static readonly CLOSED = 2;
  readyState = 1;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, ((ev: MessageEvent<string>) => void)[]>();
  constructor(public url: string) {
    FakeSource.last = this;
  }
  addEventListener(kind: string, fn: (ev: MessageEvent<string>) => void) {
    this.listeners.set(kind, [...(this.listeners.get(kind) ?? []), fn]);
  }
  close() {
    this.readyState = 2;
  }
  emit(line: string) {
    const kind = (JSON.parse(line) as { kind: string }).kind;
    for (const fn of this.listeners.get(kind) ?? []) fn(new MessageEvent(kind, { data: line }));
  }
}

beforeEach(() => {
  vi.stubGlobal("EventSource", FakeSource);
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => setTimeout(cb, 0));
  const info = {
    id: runId,
    config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 30, step_s: 120, case: "multizone_office_complex_air" },
    state: "running",
    ticks: 3,
    events: 8,
  };
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify([info]), { status: 200 })));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("RunsPage", () => {
  it("follows the newest run into charts, receipts and KPIs", async () => {
    render(<RunsPage />);
    expect(await screen.findAllByText(runId)).toBeTruthy();
    expect(FakeSource.last?.url).toBe(`/api/runs/${runId}/events`);
    await act(async () => {
      for (const line of lines) FakeSource.last?.emit(line);
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(screen.getByText("completed")).toBeTruthy();
    expect(screen.getByText("Chilled-water supply temperature")).toBeTruthy();
    expect(screen.getAllByText("chw").length).toBeGreaterThan(0);
    expect(screen.getByText("Energy use (kWh/m²)")).toBeTruthy();
    expect(screen.getByText(/Air handler requests feeding the plant reset/)).toBeTruthy();
  });

  it("re-subscribes from the start when events are missing", async () => {
    render(<RunsPage />);
    await screen.findAllByText(runId);
    const first = FakeSource.last;
    await act(async () => {
      for (const line of gapped) first?.emit(line);
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(FakeSource.last).not.toBe(first);
    expect(first?.readyState).toBe(2);
  });

  it("starting a run fetches the list a bounded number of times", async () => {
    const started = { ...JSON.parse(JSON.stringify({})), id: "run-new" };
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) =>
      init?.method === "POST"
        ? new Response(JSON.stringify({ ...started, config: { baseline: false, requests: true, start_s: 0, warmup_s: 0, steps: 1, step_s: 120, case: "c" }, state: "starting", ticks: 0, events: 0 }), { status: 201 })
        : new Response("[]", { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<RunsPage />);
    await act(async () => {
      screen.getByRole("button", { name: /Start/ }).click();
      await new Promise((r) => setTimeout(r, 200));
    });
    const gets = fetchMock.mock.calls.filter(([, init]) => (init?.method ?? "GET") === "GET").length;
    expect(gets).toBeLessThanOrEqual(3);
  });
});

describe("RunsPage with drafts", () => {
  it("starts a run on a chosen verified draft and labels it in the run", async () => {
    const draft = {
      content_id: "cxf:fnv1a128:61dcdb7e3856008f78c5a1439451caee",
      spec: "chw_plant_reset",
      scenarios: 10,
      model: "muse-spark-1.3",
      source: "recorded",
      session: "author-1",
      attempts_used: 3,
    };
    const started = {
      id: "run-9",
      config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 300, step_s: 120, case: "c", drafts: {} },
      state: "starting",
      ticks: 0,
      events: 0,
    };
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/drafts") return new Response(JSON.stringify([draft]), { status: 200 });
      if (init?.method === "POST") return new Response(JSON.stringify(started), { status: 201 });
      return new Response("[]", { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    const choice = await screen.findByRole("button", { name: /Drafted …51caee/ });
    expect(choice.textContent).toContain("(recorded)");
    act(() => choice.click());
    act(() => screen.getByRole("button", { name: /Start/ }).click());
    await vi.waitFor(() => expect(fetch.mock.calls.some(([, i]) => i?.method === "POST")).toBe(true));
    const post = fetch.mock.calls.find(([, i]) => i?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({ drafts: { chw_plant_reset: draft.content_id } });

    await vi.waitFor(() => expect(FakeSource.last?.url).toBe("/api/runs/run-9/events"));
    const sequences = [
      { name: "chw", spec: "chw_plant_reset", content_id: draft.content_id, origin: "drafted", model: "muse-spark-1.3", source: "recorded" },
      { name: "ahu1", spec: "ahu_chw_requests", content_id: "cxf:x", origin: "reference" },
    ];
    await act(async () => {
      FakeSource.last?.emit(JSON.stringify({ run_id: "run-9", seq: 0, kind: "started", config: started.config, sequences }));
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByText(/Drafted by muse-spark-1.3 \(recorded run\)/)).toBeTruthy();
    expect(screen.getByText("Hand-built reference")).toBeTruthy();
  });
});
