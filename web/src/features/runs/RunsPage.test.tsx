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

/** What the server answers when it was started without the BACnet mirror. */
function bacnetOff() {
  return new Response(JSON.stringify({ code: "bacnet-off", message: "off" }), { status: 404 });
}

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
  // The run view opens its stream in an effect, after the list has rendered: tests wait for
  // it rather than assume it, and never see a source left over from an earlier test.
  FakeSource.last = null;
  vi.stubGlobal("EventSource", FakeSource);
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => setTimeout(cb, 0));
  const info = {
    id: runId,
    config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 30, step_s: 120, case: "multizone_office_complex_air" },
    state: "running",
    ticks: 3,
    events: 8,
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      url === "/api/bacnet" ? bacnetOff() : new Response(JSON.stringify(url === "/api/runs" ? [info] : []), { status: 200 }),
    ),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("RunsPage", () => {
  it("follows the newest run into charts, receipts and KPIs", async () => {
    render(<RunsPage />);
    expect(await screen.findAllByText(runId)).toBeTruthy();
    await vi.waitFor(() => expect(FakeSource.last?.url).toBe(`/api/runs/${runId}/events`), { timeout: 5000 });
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

  it("sets the KPIs beside a recorded baseline run of the same window", async () => {
    const info = {
      id: runId,
      config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 30, step_s: 120, case: "multizone_office_complex_air" },
      state: "running",
      ticks: 3,
      events: 8,
    };
    const baseline = { name: "base", recorded_run: "run-b", ticks: 30, baseline: true, start_s: 16027200, drafted: false, state: "completed", kpis: { ener_tot: 0.03 } };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url === "/api/bacnet") return bacnetOff();
        const body = url === "/api/runs" ? [info] : url === "/api/replays" ? [baseline] : [];
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<RunsPage />);
    await screen.findAllByText(runId);
    await vi.waitFor(() => expect(FakeSource.last?.url).toBe(`/api/runs/${runId}/events`), { timeout: 5000 });
    await act(async () => {
      for (const line of lines) FakeSource.last?.emit(line);
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(await screen.findByText(/Beside the recorded baseline run run-b/)).toBeTruthy();
    // 0.02966 against 0.03; KPIs the baseline lacks read "no data", never 0.
    expect(screen.getByText("−1.1 %")).toBeTruthy();
    expect(screen.getAllByText("no data").length).toBeGreaterThan(0);
  });

  it("re-subscribes from the start when events are missing", async () => {
    render(<RunsPage />);
    await screen.findAllByText(runId);
    await vi.waitFor(() => expect(FakeSource.last).not.toBeNull(), { timeout: 5000 });
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
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) =>
      url === "/api/bacnet"
        ? bacnetOff()
        : init?.method === "POST"
        ? new Response(JSON.stringify({ ...started, config: { baseline: false, requests: true, start_s: 0, warmup_s: 0, steps: 1, step_s: 120, case: "c" }, state: "starting", ticks: 0, events: 0 }), { status: 201 })
        : new Response("[]", { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<RunsPage />);
    await act(async () => {
      screen.getByRole("button", { name: /Start/ }).click();
      await new Promise((r) => setTimeout(r, 200));
    });
    // The run list (not the drafts or replay lists) is what a reload loop would hammer.
    const gets = fetchMock.mock.calls.filter(
      ([url, init]) => url === "/api/runs" && (init?.method ?? "GET") === "GET",
    ).length;
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
      if (url === "/api/bacnet") return bacnetOff();
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
    await vi.waitFor(() => expect(fetch.mock.calls.some(([, i]) => i?.method === "POST")).toBe(true), { timeout: 5000 });
    const post = fetch.mock.calls.find(([, i]) => i?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({ drafts: { chw_plant_reset: draft.content_id } });

    await vi.waitFor(() => expect(FakeSource.last?.url).toBe("/api/runs/run-9/events"), { timeout: 5000 });
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

describe("RunsPage replays", () => {
  it("replays a recorded run and labels it as a replay", async () => {
    const replay = { name: "day185-noon-requests", recorded_run: "run-1", ticks: 300, baseline: false, start_s: 16027200, drafted: false, state: "completed" };
    const config = { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 300, step_s: 120, case: "c", drafts: {}, replay: { record: replay.name, recorded_run: "run-1" } };
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (url === "/api/replays") return new Response(JSON.stringify([replay]), { status: 200 });
      if (url === "/api/runs/replay" && init?.method === "POST")
        return new Response(JSON.stringify({ id: "run-77", config, state: "starting", ticks: 0, events: 0 }), { status: 201 });
      return new Response("[]", { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    const button = await screen.findByRole("button", { name: /Replay/ });
    act(() => button.click());
    await vi.waitFor(() => expect(FakeSource.last?.url).toBe("/api/runs/run-77/events"), { timeout: 5000 });
    const post = fetch.mock.calls.find(([u, i]) => u === "/api/runs/replay" && i?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ record: replay.name });
    await act(async () => {
      FakeSource.last?.emit(JSON.stringify({ run_id: "run-77", seq: 0, kind: "started", config, sequences: [] }));
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByText("Replay of a recorded run")).toBeTruthy();
    await act(async () => {
      FakeSource.last?.emit(JSON.stringify({ run_id: "run-77", seq: 1, kind: "kpi", values: { ener_tot: 1.5 } }));
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByText(/not recomputed by this replay/)).toBeTruthy();
    expect(screen.queryByText(/Computed by BOPTEST for this run/)).toBeNull();
    expect(screen.getByText(/Nothing runs\s+against BOPTEST now/)).toBeTruthy();
  });
});

describe("RunsPage BACnet note", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("says live runs are mirrored read-only when the mirror is on", async () => {
    const info = { device_instance: 599, port: 47808, points: [{ name: "chw.TSupSet", source: "derived", object_type: "analog-value", instance: 1, unit: "K" }] };
    vi.stubGlobal("EventSource", FakeSource);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => new Response(JSON.stringify(url === "/api/bacnet" ? info : []), { status: 200 })),
    );
    render(<RunsPage />);
    expect(await screen.findByText(/mirrored read-only over BACnet\/IP: device 599, 1 objects, UDP port 47808/)).toBeTruthy();
  });

  it("says nothing when the mirror is off", async () => {
    vi.stubGlobal("EventSource", FakeSource);
    const fetch = vi.fn(async (url: string) => (url === "/api/bacnet" ? bacnetOff() : new Response("[]", { status: 200 })));
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    await vi.waitFor(() => expect(fetch.mock.calls.some(([u]) => u === "/api/bacnet")).toBe(true));
    expect(screen.queryByText(/BACnet/)).toBeNull();
  });
});
