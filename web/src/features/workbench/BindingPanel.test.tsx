import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { format, parse } from "@/lib/route";
import type { BindingView } from "@/types/generated/BindingView";

import { BindingPanel } from "./BindingPanel";

const proposal: BindingView = {
  spec: "sat",
  task: "task:1",
  confirmed: false,
  proposed_by: "recorded",
  problems: ["input `ena` (Bool) cannot be fed as `measurement`"],
  binding: {
    floors: [1, 2, 3],
    inputs: [{ input: "ena", kind: "measurement", point: "floor{floor}_reaAHU_occ_y", threshold: 0, value: 0 }],
    outputs: [{ output: "sp", point: "floor{floor}_TSupAirSet", measured: "floor{floor}_reaAHU_TSup_y", min: 285, max: 300 }],
    notes: ["Occupancy stands in for the enable."],
    units: {},
  },
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("BindingPanel", () => {
  it("shows the labelled proposal and its problems, and confirms the edited binding", async () => {
    const calls: { url: string; body: unknown }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        calls.push({ url, body });
        if (url.endsWith("/binding/propose")) return new Response(JSON.stringify(proposal), { status: 200 });
        if (url.endsWith("/binding") && init?.method === "POST")
          return new Response(JSON.stringify({ ...proposal, problems: [], confirmed: true, proposed_by: undefined, binding: body.binding }), { status: 200 });
        return new Response('{"code":"no-binding","message":"not bound"}', { status: 404 });
      }),
    );
    const bound = vi.fn();
    render(<BindingPanel spec="sat" onBound={bound} />);
    fireEvent.click(await screen.findByRole("button", { name: /Propose a binding with Muse Spark/ }));
    expect(await screen.findByText(/Proposed by Muse Spark \(recorded\)/)).toBeTruthy();
    expect(screen.getByText(/cannot be fed as `measurement`/)).toBeTruthy();
    expect(screen.getByText("Occupancy stands in for the enable.")).toBeTruthy();
    // Each floor box says which floor it is, not just "2".
    expect((screen.getByRole("checkbox", { name: "Floor 2" }) as HTMLInputElement).checked).toBe(true);

    fireEvent.change(screen.getByLabelText("How ena is fed"), { target: { value: "above" } });
    const threshold = screen.getByLabelText("Threshold for ena");
    fireEvent.change(threshold, { target: { value: "0.5" } });
    fireEvent.blur(threshold);
    fireEvent.click(screen.getByRole("button", { name: /Confirm this binding/ }));
    await vi.waitFor(() => expect(bound).toHaveBeenCalled());
    const sent = calls.find((c) => c.url === "/api/workbench/sat/binding" && c.body)?.body as {
      binding: { inputs: { kind: string; threshold: number }[] };
    };
    expect(sent.binding.inputs[0]).toMatchObject({ kind: "above", threshold: 0.5 });
  });

  it("once bound, links to the run form with a verified draft chosen", async () => {
    const confirmed: BindingView = { ...proposal, problems: [], confirmed: true, proposed_by: undefined };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(confirmed), { status: 200 })));
    const href = format({ page: "runs", draft: { spec: "sat", contentId: "cxf:fnv1a128:abc" } });
    render(<BindingPanel spec="sat" onBound={() => {}} next={{ label: "Run a verified draft", href }} />);
    expect(await screen.findByText("Bound to the building")).toBeTruthy();
    const link = screen.getByRole("link", { name: /Run a verified draft/ });
    expect(link.getAttribute("href")).toBe("#/runs?spec=sat&draft=cxf%3Afnv1a128%3Aabc");
    // The link lands on the run form with that draft preset.
    expect(parse(href)).toEqual({ page: "runs", draft: { spec: "sat", contentId: "cxf:fnv1a128:abc" } });
  });
});
