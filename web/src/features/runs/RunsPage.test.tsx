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

/** What the server answers for a run's ledger before (or without) BOPTEST's KPIs. */
function noLedger() {
  return new Response(JSON.stringify({ code: "no-kpis", message: "no KPIs" }), { status: 409 });
}

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
      url === "/api/bacnet"
        ? bacnetOff()
        : url.includes("/ledger")
          ? noLedger()
          : new Response(JSON.stringify(url === "/api/runs" ? [info] : []), { status: 200 }),
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
    // Instances are grouped by spec and graph; each instance is one click away.
    expect(screen.getAllByText("chw_plant_reset").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /Show all \d+ instances/ })).toBeTruthy();
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
    const baseline = { name: "base", recorded_run: "run-b", ticks: 30, baseline: true, start_s: 16027200, case: "multizone_office_complex_air", step_s: 120, drafted: false, drafts: [], pressure: false, own: [], state: "completed", kpis: { ener_tot: 0.03 } };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url === "/api/bacnet") return bacnetOff();
        if (url.includes("/ledger")) return noLedger();
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
    // In the headline tile and in the table row.
    expect(screen.getAllByText("−1.1 %").length).toBe(2);
    expect(screen.getByLabelText("Change against the baseline run")).toBeTruthy();
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

describe("RunsPage without BOPTEST", () => {
  it("disables Start with a plain reason and keeps replays", async () => {
    const replay = { name: "demo", recorded_run: "run-r", ticks: 300, baseline: false, start_s: 16027200, drafted: false, drafts: [], pressure: false, own: [], state: "completed" };
    const fetch = vi.fn(async (url: string) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (url === "/api/health")
        return new Response(JSON.stringify({ status: "ok", engine_rev: "x", specs: 4, recorded_model: true, boptest: false }), { status: 200 });
      return new Response(JSON.stringify(url === "/api/replays" ? [replay] : []), { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    expect(await screen.findByText(/No BOPTEST configured on this server/)).toBeTruthy();
    expect((screen.getByRole("button", { name: /^Start$/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(await screen.findByRole("button", { name: "Replay demo" })).toBeTruthy();
  });

  it("asks for the replay speed chosen", async () => {
    const replay = { name: "demo", recorded_run: "run-r", ticks: 300, baseline: false, start_s: 16027200, drafted: false, drafts: [], pressure: false, own: [], state: "completed" };
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (init?.method === "POST")
        return new Response(JSON.stringify({ code: "bad-request", message: "not now" }), { status: 400 });
      return new Response(JSON.stringify(url === "/api/replays" ? [replay] : []), { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    const button = await screen.findByRole("button", { name: "Replay demo" });
    act(() => screen.getByRole("button", { name: "10×" }).click());
    act(() => button.click());
    await vi.waitFor(() => expect(fetch.mock.calls.some(([, i]) => i?.method === "POST")).toBe(true), { timeout: 5000 });
    const post = fetch.mock.calls.find(([, i]) => i?.method === "POST");
    expect(post?.[0]).toBe("/api/runs/replay");
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ record: "demo", speed: 10 });
  });
});

describe("RunsPage test faults", () => {
  it("sends the kind of fault chosen: a stuck valve reading", async () => {
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (init?.method === "POST")
        return new Response(JSON.stringify({ code: "bad-request", message: "not now" }), { status: 400 });
      return new Response("[]", { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    act(() => screen.getByRole("button", { name: /Change/ }).click());
    act(() => screen.getByRole("switch", { name: /Inject a test fault/ }).click());
    const stuck = await screen.findByRole("button", { name: "Stuck valve reading" });
    act(() => stuck.click());
    expect(stuck.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByText(/held at 100 % open for 60 steps/)).toBeTruthy();
    act(() => screen.getByRole("button", { name: /Start/ }).click());
    await vi.waitFor(() => expect(fetch.mock.calls.some(([, i]) => i?.method === "POST")).toBe(true), { timeout: 5000 });
    const post = fetch.mock.calls.find(([, i]) => i?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({
      fault: { point: "floor1_reaAHU_yCooVal_y", from_step: 30, steps: 60, mode: { kind: "hold", value: 1 } },
    });
  });
});

describe("RunsPage fault cost preset", () => {
  it("runs the demo window with floor 1's supply-air reading 4 K high from step 30", async () => {
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (init?.method === "POST")
        return new Response(JSON.stringify({ code: "bad-request", message: "not now" }), { status: 400 });
      return new Response("[]", { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage />);
    act(() => screen.getByRole("button", { name: "Fault cost" }).click());
    act(() => screen.getByRole("button", { name: /Start/ }).click());
    await vi.waitFor(() => expect(fetch.mock.calls.some(([, i]) => i?.method === "POST")).toBe(true), { timeout: 5000 });
    const post = fetch.mock.calls.find(([, i]) => i?.method === "POST");
    // The same window as the Demo window preset, so a healthy run of it is the comparison.
    expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({
      start_s: 185 * 86400 + 12 * 3600,
      steps: 300,
      fault: { point: "floor1_reaAHU_TSup_y", from_step: 30, steps: 60, mode: { kind: "bias", offset: 4 } },
    });
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
      stored_seq: 1,
    };
    const started = {
      id: "run-9",
      config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 300, step_s: 120, case: "c", drafts: {}, bindings: {} },
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
    // Said once, in the detail line.
    expect(choice.textContent?.match(/recorded/g)).toHaveLength(1);
    expect(choice.textContent).toContain("recorded session");
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
    expect(screen.getByText(/Drafted by muse-spark-1.3 \(recorded session\)/)).toBeTruthy();
    expect(screen.getByText("Hand-built reference")).toBeTruthy();
  });
});

describe("RunsPage draft order", () => {
  it("offers the newest drafts by store order, not by content id", async () => {
    // Content ids are hashes: here they sort the opposite way to store order.
    const drafts = [5, 4, 3, 2, 1].map((n) => ({
      content_id: `cxf:fnv1a128:00000${n}`,
      spec: "chw_plant_reset",
      scenarios: 10,
      model: "muse-spark-1.3",
      source: "live",
      session: `author-${n}`,
      attempts_used: 1,
      stored_seq: 6 - n,
    }));
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url === "/api/bacnet") return bacnetOff();
        if (url === "/api/drafts") return new Response(JSON.stringify(drafts), { status: 200 });
        return new Response("[]", { status: 200 });
      }),
    );
    render(<RunsPage />);
    const group = await screen.findByRole("group", { name: "Plant reset graph" });
    const shown = Array.from(group.querySelectorAll("button[title]")).map((b) => b.getAttribute("title"));
    // stored_seq 5, 4, 3 are content ids ending 1, 2, 3.
    expect(shown).toEqual(["cxf:fnv1a128:000001", "cxf:fnv1a128:000002", "cxf:fnv1a128:000003"]);
  });
});

describe("RunsPage replays", () => {
  it("replays a recorded run and labels it as a replay", async () => {
    const replay = { name: "day185-noon-requests", recorded_run: "run-1", ticks: 300, baseline: false, start_s: 16027200, drafted: false, drafts: [], pressure: false, own: [], state: "completed" };
    const config = { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 300, step_s: 120, case: "c", drafts: {}, bindings: {}, replay: { record: replay.name, recorded_run: "run-1" } };
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/bacnet") return bacnetOff();
      if (url.includes("/ledger")) return noLedger();
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

  it("points out a linked record, grouped by window, without starting it", async () => {
    const mine = { name: "day185-noon-your-sequence", recorded_run: "run-3", ticks: 300, baseline: false, start_s: 16027200, drafted: true, drafts: ["ahu1_sat_stpt"], pressure: false, own: ["ahu1_sat_stpt"], state: "completed" };
    const early = { ...mine, name: "day143-morning-baseline", start_s: 12376800, baseline: true, drafted: false, drafts: [], own: [] };
    const fetch = vi.fn(async (url: string) => {
      if (url === "/api/bacnet") return bacnetOff();
      return new Response(JSON.stringify(url === "/api/replays" ? [mine, early] : []), { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<RunsPage replay="day185-noon-your-sequence" />);
    const groups = await screen.findAllByRole("region");
    expect(groups.map((g) => g.getAttribute("aria-label"))).toEqual(["Day 143 morning", "Day 185 noon"]);
    const item = screen.getByText("Your sequence ahu1_sat_stpt (Spark draft); plant reset hand-built").closest("li");
    expect(item?.getAttribute("aria-current")).toBe("true");
    expect(item?.textContent).toContain("day185-noon-your-sequence");
    await new Promise((r) => setTimeout(r, 20));
    expect(fetch.mock.calls.some(([u]) => u === "/api/runs/replay")).toBe(false);
  });

  it("lists runs above the replays, each described in words", async () => {
    const replay = { name: "day185-noon-requests", recorded_run: "run-1", ticks: 300, baseline: false, start_s: 16027200, drafted: false, drafts: [], pressure: false, own: [], state: "completed" };
    const info = {
      id: "run-5",
      config: { baseline: false, requests: true, pressure: false, start_s: 16027200, warmup_s: 86400, steps: 30, step_s: 120, case: "c", drafts: { chw_plant_reset: "cxf:1" }, bindings: {} },
      state: "completed",
      ticks: 30,
      events: 40,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) =>
        url === "/api/bacnet"
          ? bacnetOff()
          : new Response(JSON.stringify(url === "/api/runs" ? [info] : url === "/api/replays" ? [replay] : []), { status: 200 }),
      ),
    );
    render(<RunsPage />);
    const runs = await screen.findByRole("navigation", { name: "Runs" });
    const link = screen.getByRole("link", { name: /Spark-drafted plant reset, chilled water/ });
    expect(link.getAttribute("href")).toBe("#/runs/run-5");
    const replays = await screen.findByText("Replay a recorded run");
    expect(runs.compareDocumentPosition(replays) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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

describe("RunsPage run not on the server", () => {
  it("says a linked run is gone after a restart, without a Stop button", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => (url === "/api/bacnet" ? bacnetOff() : new Response("[]", { status: 200 }))),
    );
    render(<RunsPage run="run-gone" />);
    expect(await screen.findByText(/This run is not on this server \(runs are kept in memory/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Stop run/ })).toBeNull();
    expect(screen.queryByText(/Waiting for the run to start/)).toBeNull();
  });
});

describe("RunsPage gap banner", () => {
  it("does not carry a gap banner over to another run", async () => {
    const other = "run-other";
    const info = (id: string) => ({
      id,
      config: { baseline: false, requests: true, start_s: 16027200, warmup_s: 86400, steps: 30, step_s: 120, case: "c" },
      state: "running",
      ticks: 3,
      events: 8,
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) =>
        url === "/api/bacnet"
          ? bacnetOff()
          : url.includes("/ledger")
            ? noLedger()
            : new Response(JSON.stringify(url === "/api/runs" ? [info(runId), info(other)] : []), { status: 200 }),
      ),
    );
    const { rerender } = render(<RunsPage run={runId} />);
    // Each gap re-subscribes; past the limit the banner stays up.
    for (let i = 0; i < 4; i++) {
      await vi.waitFor(() => expect(FakeSource.last?.readyState).toBe(1), { timeout: 5000 });
      const source = FakeSource.last;
      await act(async () => {
        for (const line of gapped) source?.emit(line);
        await new Promise((r) => setTimeout(r, 10));
      });
    }
    expect(await screen.findByText(/Events are missing from this stream/)).toBeTruthy();
    rerender(<RunsPage run={other} />);
    await vi.waitFor(() => expect(FakeSource.last?.url).toBe(`/api/runs/${other}/events`), { timeout: 5000 });
    expect(screen.queryByText(/Events are missing from this stream/)).toBeNull();
  });
});
