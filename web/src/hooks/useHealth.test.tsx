import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { HEALTH_REFRESH_MS, useHealth } from "./useHealth";
import { useLiveRuns } from "./useLiveRuns";

function Header() {
  const health = useHealth();
  return <p>{health.status === "ready" ? `boptest ${String(health.data.boptest)}` : health.status}</p>;
}

function Form() {
  const live = useLiveRuns();
  return <p>{live.available ? "live on" : "live off"}</p>;
}

const healthBody = (boptest: boolean) =>
  new Response(JSON.stringify({ status: "ok", engine_rev: "x", specs: 4, recorded_model: true, boptest }), { status: 200 });

beforeEach(() => vi.useFakeTimers());

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("useHealth", () => {
  it("shares one request between its readers and asks again on a timer", async () => {
    let boptest = true;
    const fetch = vi.fn(async () => healthBody(boptest));
    vi.stubGlobal("fetch", fetch);
    render(
      <>
        <Header />
        <Form />
      </>,
    );
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(screen.getByText("boptest true")).toBeTruthy();
    expect(fetch).toHaveBeenCalledTimes(1);
    // The server is restarted without BOPTEST: both readers see it at the next refresh.
    boptest = false;
    await act(() => vi.advanceTimersByTimeAsync(HEALTH_REFRESH_MS));
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(screen.getByText("boptest false")).toBeTruthy();
    expect(screen.getByText("live off")).toBeTruthy();
  });

  it("stops asking once nothing reads it", async () => {
    const fetch = vi.fn(async () => healthBody(true));
    vi.stubGlobal("fetch", fetch);
    const { unmount } = render(<Header />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    unmount();
    await act(() => vi.advanceTimersByTimeAsync(HEALTH_REFRESH_MS * 3));
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
