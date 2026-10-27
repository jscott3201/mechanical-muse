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
});
