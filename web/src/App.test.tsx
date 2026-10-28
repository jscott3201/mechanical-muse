import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
  about: "Requests from one air handler.",
  description: "The task text given to the model.",
  params: [{ name: "persistence", unit: "s", meaning: "How long a condition must last", value: 120 }],
  scenarios: [
    { name: "steady_normal", description: "No requests." },
    { name: "valve_latch", description: "Latch." },
  ],
  step_s: 60,
  reference: true,
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
const health: Health = { status: "ok", engine_rev: "4063202b9a9e50c7", specs: 1, recorded_model: true, boptest: true };

const routes: Record<string, unknown> = {
  "GET /api/health": health,
  "GET /api/specs": [summary],
  "GET /api/workbench": [],
  "GET /api/specs/ahu_chw_requests": detail,
  "POST /api/specs/ahu_chw_requests/verify": receipt,
};

/** Answers from `table`, 404 for anything else. */
function serve(table: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const body = table[`${init?.method ?? "GET"} ${url}`];
      return body === undefined
        ? new Response('{"code":"not-found","message":"no such route"}', { status: 404 })
        : new Response(JSON.stringify(body), { status: 200 });
    }),
  );
}

/** Radix tabs switch on mouse down, not click. */
function openTab(name: RegExp) {
  fireEvent.mouseDown(screen.getByRole("tab", { name }), { button: 0 });
}

beforeEach(() => {
  // Each test starts on the overview: the hash is the app's route.
  window.location.hash = "";
});

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
    expect(screen.getByText("recorded model only")).toBeTruthy();
    // The app opens on the overview; its first action leads to the sequences.
    fireEvent.click(screen.getByRole("button", { name: /Explore sequences/ }));
    fireEvent.click(await screen.findByRole("link", { name: /Air handler chilled-water requests/ }));
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
    fireEvent.click(screen.getByRole("button", { name: /Explore sequences/ }));
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
    fireEvent.click(screen.getByRole("button", { name: /Explore sequences/ }));
    fireEvent.click(await screen.findByRole("link", { name: /Air handler chilled-water requests/ }));
    fireEvent.click(await screen.findByRole("button", { name: /Verify/ }));
    // Switch specs while the first verification is still running, then let it answer.
    fireEvent.click(screen.getByRole("link", { name: /Chilled-water plant reset/ }));
    await screen.findByRole("heading", { name: "Chilled-water plant reset" }).catch(() => undefined);
    releaseVerify(new Response(JSON.stringify(receipt), { status: 200 }));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByText("Not verified")).toBeNull();
    expect(screen.queryByText(/passes 1 of 2/)).toBeNull();
  });
});

describe("App routes", () => {
  it("opens a spec's section straight from the hash and names the page", async () => {
    serve(routes);
    window.location.hash = "#/sequences/ahu_chw_requests/scenarios";
    render(<App />);
    expect(await screen.findByText("Latch.")).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Test scenarios/ }).getAttribute("aria-selected")).toBe("true");
    await vi.waitFor(() => expect(document.title).toBe("Air handler chilled-water requests · Mechanical Muse"));
    // A sub-tab click is a route too, so Back returns to the scenarios.
    fireEvent.mouseDown(screen.getByRole("tab", { name: /Task text/ }), { button: 0 });
    await vi.waitFor(() => expect(window.location.hash).toBe("#/sequences/ahu_chw_requests/task"));
  });

  it("keeps a tab's state when another tab is opened and closed", async () => {
    serve(routes);
    window.location.hash = "#/sequences/ahu_chw_requests";
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /Verify/ }));
    expect(await screen.findByText("Not verified")).toBeTruthy();
    openTab(/Runs/);
    await vi.waitFor(() => expect(window.location.hash).toBe("#/runs"));
    await vi.waitFor(() => expect(document.title).toBe("Runs · Mechanical Muse"));
    openTab(/Sequences/);
    // Back on the same spec, with the receipt it had: the tab was hidden, not unmounted.
    await vi.waitFor(() => expect(window.location.hash).toBe("#/sequences/ahu_chw_requests"));
    expect(screen.getByText("Not verified")).toBeTruthy();
  });

  it("labels an engineer's recorded run as theirs and links it to the Runs page", async () => {
    const own = { name: "day185-noon-your-sequence", recorded_run: "run-3", ticks: 300, baseline: false, start_s: 16027200, drafted: true, drafts: ["ahu1_sat_stpt"], pressure: false, own: ["ahu1_sat_stpt"], state: "completed", kpis: { ener_tot: 0.3 } };
    const base = { ...own, name: "day185-noon-baseline", baseline: true, drafted: false, drafts: [], own: [], kpis: { ener_tot: 0.31 } };
    serve({ ...routes, "GET /api/replays": [own, base] });
    render(<App />);
    const link = await screen.findByRole("link", { name: "Your sequence ahu1_sat_stpt (Spark draft); plant reset hand-built" });
    expect(screen.queryByText(/Spark-drafted plant reset/)).toBeNull();
    // It points the record out on the Runs page; nothing starts by itself.
    expect(link.getAttribute("href")).toBe("#/runs?replay=day185-noon-your-sequence");
  });

  it("leads from the overview to the workbench", async () => {
    serve(routes);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Bring your own sequence/ }));
    await vi.waitFor(() => expect(window.location.hash).toBe("#/workbench"));
    await vi.waitFor(() => expect(screen.getByRole("tab", { name: /Bring your own/ }).getAttribute("aria-selected")).toBe("true"));
  });
});

describe("An engineer's own sequence", () => {
  it("shows its stored verified drafts in place of a hand-built graph and runs one", async () => {
    const own: SpecSummary = { ...summary, name: "sat", title: "Supply air reset" };
    const draft = { content_id: "cxf:fnv1a128:abc", spec: "sat", scenarios: 2, model: "muse-spark-1.3", source: "recorded", session: "author-1", attempts_used: 2 };
    const other = { ...draft, content_id: "cxf:fnv1a128:zzz", spec: "chw_plant_reset" };
    const binding = { spec: "sat", task: "task:1", confirmed: true, problems: [], binding: { floors: [1], inputs: [], outputs: [], notes: [], units: {} } };
    serve({
      ...routes,
      "GET /api/workbench": [own],
      "GET /api/specs/sat": { ...detail, summary: own, reference: false },
      "GET /api/drafts": [other, draft],
      "GET /api/workbench/sat/binding": binding,
    });
    window.location.hash = "#/sequences/sat/graph";
    render(<App />);
    expect(await screen.findByText("Verified drafts")).toBeTruthy();
    expect(screen.getByText("cxf:fnv1a128:abc")).toBeTruthy();
    // Only this sequence's drafts, each labelled with where it came from.
    expect(screen.queryByText("cxf:fnv1a128:zzz")).toBeNull();
    expect(within(screen.getByRole("list", { name: "Stored drafts" })).getByRole("button", { pressed: true }).textContent).toContain("recorded");
    // The journey bar reads its steps from the same data.
    expect(await screen.findByRole("link", { name: "Run (next)" })).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: /Run this draft on BOPTEST/ }));
    await vi.waitFor(() => expect(window.location.hash).toBe("#/runs?spec=sat&draft=cxf%3Afnv1a128%3Aabc"));
  });
});
