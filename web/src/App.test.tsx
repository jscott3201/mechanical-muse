import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Health } from "@/types/generated/Health";
import type { Receipt } from "@/types/generated/Receipt";
import type { SpecDetail } from "@/types/generated/SpecDetail";
import type { SpecSummary } from "@/types/generated/SpecSummary";

import { App } from "./App";

const summary: SpecSummary = {
  name: "ahu_chw_requests",
  title: "Air handler chilled-water requests",
  inputs: [{ name: "uCooCoi", type: "Real", unit: "1", meaning: "Cooling coil valve position" }],
  outputs: [{ name: "yChiPlaReq", type: "Integer", unit: "1", meaning: "Chiller plant request" }],
  scenarios: 2,
};
const detail: SpecDetail = {
  summary,
  description: "Requests from one air handler.",
  params: [{ name: "persistence", unit: "s", meaning: "How long a condition must last", value: 120 }],
  scenarios: [
    { name: "steady_normal", description: "No requests." },
    { name: "valve_latch", description: "Latch." },
  ],
  step_s: 60,
};
const receipt: Receipt = {
  spec: "ahu_chw_requests",
  content_id: "cxf:fnv1a128:abc",
  passed: false,
  scenarios: [
    { name: "steady_normal", ticks: 21, result: "pass" },
    {
      name: "valve_latch",
      ticks: 9,
      result: "fail",
      t: 480,
      output: "yChiWatResReq",
      expected: 1,
      actual: 0,
      from_s: 300,
      to_s: 1440,
    },
  ],
};
const health: Health = { status: "ok", engine_rev: "4063202b9a9e50c7", specs: 1 };

const routes: Record<string, unknown> = {
  "GET /api/health": health,
  "GET /api/specs": [summary],
  "GET /api/specs/ahu_chw_requests": detail,
  "POST /api/specs/ahu_chw_requests/verify": receipt,
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("lists specs, opens one and shows a failing receipt honestly", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        const body = routes[`${init?.method ?? "GET"} ${url}`];
        return body === undefined
          ? new Response('{"code":"not-found","message":"no such route"}', { status: 404 })
          : new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    expect(await screen.findByText("engine 4063202")).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: /Air handler chilled-water requests/ }));
    expect(await screen.findByText("Requests from one air handler.")).toBeTruthy();
    expect(screen.getByText("120 s")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Verify/ }));
    expect(await screen.findByText("Not verified")).toBeTruthy();
    expect(screen.getByText("passes 1 of 2 scenarios")).toBeTruthy();
    expect(screen.getByText(/expected 1, got 0/)).toBeTruthy();
  });

  it("says when the server is offline", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("fetch failed"))));
    render(<App />);
    expect(await screen.findByText("server offline")).toBeTruthy();
    expect(await screen.findByText("network")).toBeTruthy();
  });

  it("never shows one spec's receipt under another", async () => {
    const other: SpecSummary = { ...summary, name: "chw_plant_reset", title: "Chilled-water plant reset" };
    let releaseVerify: (r: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        const key = `${init?.method ?? "GET"} ${url}`;
        if (key === "POST /api/specs/ahu_chw_requests/verify") {
          return new Promise<Response>((resolve) => {
            releaseVerify = resolve;
          });
        }
        const table: Record<string, unknown> = {
          ...routes,
          "GET /api/specs": [summary, other],
          "GET /api/specs/chw_plant_reset": { ...detail, summary: other },
        };
        const body = table[key];
        return body === undefined
          ? new Response('{"code":"not-found","message":"no such route"}', { status: 404 })
          : new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /Air handler chilled-water requests/ }));
    fireEvent.click(await screen.findByRole("button", { name: /Verify/ }));
    // Switch specs while the first verification is still running, then let it answer.
    fireEvent.click(screen.getByRole("button", { name: /Chilled-water plant reset/ }));
    await screen.findByRole("heading", { name: "Chilled-water plant reset" }).catch(() => undefined);
    releaseVerify(new Response(JSON.stringify(receipt), { status: 200 }));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByText("Not verified")).toBeNull();
    expect(screen.queryByText(/passes 1 of 2/)).toBeNull();
  });
});
