import { act, cleanup, render, screen, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { GraphView } from "@/types/generated/GraphView";

import { GraphCanvas } from "./GraphCanvas";

const graph = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "../../../../fixtures/graphs/ahu_chw_requests.json"), "utf8"),
) as GraphView;

beforeEach(() => {
  // jsdom has no layout engine; React Flow only needs these to exist.
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("GraphCanvas", () => {
  it("names each signal type in words and offers every connection as a table", () => {
    render(<GraphCanvas graph={graph} label="test graph" />);
    expect(screen.getByRole("list", { name: "Signal types" }).textContent).toMatch(/Real.*Integer.*Boolean/);
    act(() => screen.getByRole("button", { name: "Show as table" }).click());
    const rows = within(screen.getByRole("table", { name: "Connections" })).getAllByRole("row");
    expect(rows).toHaveLength(graph.connections.length + 1);
    const first = graph.connections[0];
    expect(first).toBeDefined();
    if (first) {
      const from = first.from.port ? `${first.from.node}.${first.from.port}` : first.from.node;
      expect(screen.getAllByText(from).length).toBeGreaterThan(0);
    }
    act(() => screen.getByRole("button", { name: "Show as graph" }).click());
    expect(screen.queryAllByRole("row")).toHaveLength(0);
  });

  it("lists blocks with their parameters and problems, which the canvas shows only on hover", () => {
    const withParams = graph.blocks.find((b) => b.params.length > 0);
    const target = graph.blocks[0];
    expect(withParams).toBeDefined();
    expect(target).toBeDefined();
    if (!withParams || !target) return;
    render(<GraphCanvas graph={graph} label="test graph" flagged={[target.id]} />);
    act(() => screen.getByRole("button", { name: "Show as table" }).click());
    const blocks = within(screen.getByRole("table", { name: "Blocks" }));
    expect(blocks.getAllByRole("row")).toHaveLength(graph.blocks.length + 1);
    const param = withParams.params[0];
    if (param) expect(blocks.getAllByText(new RegExp(`${param.name} = `)).length).toBeGreaterThan(0);
    const flaggedRow = blocks.getByText(target.id).closest("tr");
    expect(flaggedRow?.textContent).toContain("(has a problem)");
    expect(blocks.getAllByText("(has a problem)")).toHaveLength(1);
  });
});
