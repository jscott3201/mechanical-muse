import { act, renderHook } from "@testing-library/react";
import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useRunStream } from "./useRunStream";

const lines = readFileSync(
  path.resolve(import.meta.dirname, "../../../fixtures/runs/live-chw-requests.jsonl"),
  "utf8",
)
  .trim()
  .split("\n");

/** A stand-in EventSource that the test drives by hand. */
class FakeSource {
  static instances: FakeSource[] = [];
  static readonly CLOSED = 2;
  readyState = 1;
  url: string;
  closed = false;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, ((ev: MessageEvent<string>) => void)[]>();

  constructor(url: string) {
    this.url = url;
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
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("useRunStream", () => {
  it("follows a run and closes after it ends", async () => {
    const { result } = renderHook(() => useRunStream("run-1"));
    const source = FakeSource.instances.at(-1);
    expect(source?.url).toBe("/api/runs/run-1/events");
    await act(async () => {
      for (const line of lines.slice(0, 8)) source?.emit(line);
      await vi.runAllTimersAsync();
    });
    expect(result.current.ticks).toHaveLength(3);
    expect(result.current.phase).toBe("running");
    expect(source?.closed).toBe(false);
    await act(async () => {
      for (const line of lines.slice(8)) source?.emit(line);
      await vi.runAllTimersAsync();
    });
    expect(result.current.ended?.state).toBe("completed");
    expect(result.current.connection).toBe("closed");
    expect(source?.closed).toBe(true);
  });

  it("closes the source on unmount and opens nothing without a run", () => {
    const { unmount } = renderHook(() => useRunStream("run-2"));
    const source = FakeSource.instances.at(-1);
    unmount();
    expect(source?.closed).toBe(true);
    const before = FakeSource.instances.length;
    renderHook(() => useRunStream(null));
    expect(FakeSource.instances.length).toBe(before);
  });

  it("reports closed when the server ends the stream (204 after the end)", () => {
    const { result } = renderHook(() => useRunStream("run-3"));
    const source = FakeSource.instances.at(-1);
    act(() => {
      source?.close();
      source?.onerror?.();
    });
    expect(result.current.connection).toBe("closed");
  });

  it("reports reconnecting on a transient error", () => {
    const { result } = renderHook(() => useRunStream("run-4"));
    const source = FakeSource.instances.at(-1);
    act(() => source?.onerror?.());
    expect(result.current.connection).toBe("reconnecting");
  });

  it("switching runs closes the old stream and starts from a clean state", async () => {
    const { result, rerender } = renderHook(({ id }: { id: string | null }) => useRunStream(id), {
      initialProps: { id: "run-a" as string | null },
    });
    const first = FakeSource.instances.at(-1);
    await act(async () => {
      for (const line of lines.slice(0, 6)) first?.emit(line);
      await vi.runAllTimersAsync();
    });
    expect(result.current.ticks).toHaveLength(1);
    rerender({ id: "run-b" });
    expect(first?.closed).toBe(true);
    expect(FakeSource.instances.at(-1)?.url).toBe("/api/runs/run-b/events");
    expect(result.current.ticks).toHaveLength(0);
    rerender({ id: null });
    expect(result.current.connection).toBe("idle");
  });
});
