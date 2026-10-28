import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AuthorPanel } from "./AuthorPanel";

const lines = readFileSync(
  path.resolve(import.meta.dirname, "../../../../fixtures/authoring/chw_plant_reset-recorded.jsonl"),
  "utf8",
)
  .trim()
  .split("\n");
const first = JSON.parse(lines[0] ?? "{}") as { session_id: string; config: unknown };
const sessionId = first.session_id;
const config = first.config;

class FakeSource {
  static instances: FakeSource[] = [];
  static readonly CLOSED = 2;
  readyState = 1;
  closed = false;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, ((ev: MessageEvent<string>) => void)[]>();
  constructor(public url: string) {
    FakeSource.instances.push(this);
  }
  addEventListener(kind: string, fn: (ev: MessageEvent<string>) => void) {
    this.listeners.set(kind, [...(this.listeners.get(kind) ?? []), fn]);
  }
  close() {
    this.closed = true;
    this.readyState = FakeSource.CLOSED;
  }
  emit(line: string) {
    const kind = (JSON.parse(line) as { kind: string }).kind;
    for (const fn of this.listeners.get(kind) ?? []) fn(new MessageEvent(kind, { data: line }));
  }
}

beforeEach(() => {
  FakeSource.instances = [];
  vi.stubGlobal("EventSource", FakeSource);
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => setTimeout(cb, 0));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AuthorPanel", () => {
  it("asks for the chosen source and shows a recorded session red, amber, green", async () => {
    const fetch = vi.fn(async (url: string, _init?: RequestInit) =>
      url === "/api/health"
        ? new Response(JSON.stringify({ status: "ok", engine_rev: "fa5cb19", specs: 4, recorded_model: true, boptest: true }), { status: 200 })
        : new Response(JSON.stringify({ id: sessionId, config, state: "running", events: 0 }), { status: 201 }),
    );
    vi.stubGlobal("fetch", fetch);
    const onRunDraft = vi.fn();
    render(<AuthorPanel spec="chw_plant_reset" scenarios={10} onRunDraft={onRunDraft} />);
    // No model key on this server: live is offered but disabled, and recorded is the default.
    await waitFor(() =>
      expect((screen.getByRole("button", { name: /Live model call/ }) as HTMLButtonElement).disabled).toBe(true),
    );
    expect(screen.getByRole("button", { name: /Recorded run/ }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: /^Draft$/ }));
    await screen.findByRole("button", { name: /Authoring/ });
    const call = fetch.mock.calls.find(([u]) => u === "/api/specs/chw_plant_reset/author");
    const [url, init] = call as unknown as [string, RequestInit];
    expect(url).toBe("/api/specs/chw_plant_reset/author");
    expect(JSON.parse(String(init.body))).toEqual({ source: "recorded" });

    await waitFor(() => expect(FakeSource.instances).toHaveLength(1), { timeout: 5000 });
    const source = FakeSource.instances.at(-1);
    expect(source?.url).toBe(`/api/authoring/${sessionId}/events`);
    await act(async () => {
      for (const line of lines.slice(0, 2)) source?.emit(line);
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByText(/Muse Spark is drafting/)).toBeTruthy();
    expect(screen.getByText("Recorded")).toBeTruthy();
    expect(screen.getByText(/played back/)).toBeTruthy();

    await act(async () => {
      for (const line of lines.slice(2)) source?.emit(line);
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByText(/Refused by the engine at schedule/)).toBeTruthy();
    // The refusal's code, then one group naming the blocks in the loop, messages folded away.
    expect(screen.getAllByText("algebraic-loop", { selector: "code" })).toHaveLength(2);
    expect(screen.getByText("limiter", { selector: "span" })).toBeTruthy();
    expect(screen.getByText(/\d+ engine messages/)).toBeTruthy();
    expect(screen.getByText(/passes 8 of 10 scenarios/)).toBeTruthy();
    expect(screen.getByText(/Passes 10 of 10 scenarios/)).toBeTruthy();
    expect(screen.getByText("Verified")).toBeTruthy();
    expect(screen.getByText(/effort as recorded/)).toBeTruthy();
    expect(screen.getByText(/reasoning\), as recorded/)).toBeTruthy();
    expect(source?.closed).toBe(true);
    expect(screen.getByRole("button", { name: /^Draft$/ })).toBeTruthy();
    // The verified draft can go straight to a run.
    fireEvent.click(screen.getByRole("button", { name: /Run this draft on BOPTEST/ }));
    expect(onRunDraft).toHaveBeenCalledWith("chw_plant_reset", expect.stringMatching(/^cxf:/));
  });

  it("shows the server's refusal as text", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({ code: "bad-request", message: "no model API key is configured" }), {
          status: 400,
        }),
      ),
    );
    render(<AuthorPanel spec="chw_plant_reset" scenarios={10} />);
    fireEvent.click(screen.getByRole("button", { name: /^Draft$/ }));
    expect(await screen.findByText("no model API key is configured")).toBeTruthy();
    expect(FakeSource.instances).toHaveLength(0);
  });

  it("asks for Spark's own checks when the switch is on", async () => {
    const fetch = vi.fn(async (url: string, _init?: RequestInit) =>
      url === "/api/health"
        ? new Response(JSON.stringify({ status: "ok", engine_rev: "fa5cb19", specs: 4, recorded_model: true, boptest: true }), { status: 200 })
        : new Response(JSON.stringify({ id: sessionId, config, state: "running", events: 0 }), { status: 201 }),
    );
    vi.stubGlobal("fetch", fetch);
    render(<AuthorPanel spec="chw_plant_reset" scenarios={10} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Recorded run/ }).getAttribute("aria-pressed")).toBe("true"),
    );
    fireEvent.click(screen.getByRole("switch", { name: /Let Spark check its work/ }));
    fireEvent.click(screen.getByRole("button", { name: /^Draft$/ }));
    await screen.findByRole("button", { name: /Authoring/ });
    const call = fetch.mock.calls.find(([u]) => u === "/api/specs/chw_plant_reset/author");
    const [, init] = call as unknown as [string, RequestInit];
    expect(JSON.parse(String(init.body))).toEqual({ source: "recorded", tools: true });
  });

  it("stops waiting when the session's stream closes without an end", async () => {
    const fetch = vi.fn(async (url: string, _init?: RequestInit) =>
      url === "/api/health"
        ? new Response(JSON.stringify({ status: "ok", engine_rev: "fa5cb19", specs: 4, recorded_model: true, boptest: true }), { status: 200 })
        : new Response(JSON.stringify({ id: sessionId, config, state: "running", events: 0 }), { status: 201 }),
    );
    vi.stubGlobal("fetch", fetch);
    render(<AuthorPanel spec="chw_plant_reset" scenarios={10} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Recorded run/ }).getAttribute("aria-pressed")).toBe("true"),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Draft$/ }));
    await waitFor(() => expect(FakeSource.instances).toHaveLength(1), { timeout: 5000 });
    const source = FakeSource.instances.at(-1);
    await act(async () => {
      source?.emit(lines[0] ?? "");
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(screen.getByRole("button", { name: /Authoring/ })).toBeTruthy();
    // A restarted server forgets the session: the browser's EventSource gives up (404).
    act(() => {
      if (source) source.readyState = FakeSource.CLOSED;
      source?.onerror?.();
    });
    expect(screen.getByRole("alert").textContent).toMatch(/stream closed before it ended/);
    expect((screen.getByRole("button", { name: /^Draft$/ }) as HTMLButtonElement).disabled).toBe(false);
  });
});

