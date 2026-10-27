import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import type { GraphView } from "@/types/generated/GraphView";

import { layout } from "./layout";

const load = (name: string) =>
  JSON.parse(
    readFileSync(path.resolve(import.meta.dirname, `../../../../fixtures/graphs/${name}.json`), "utf8"),
  ) as GraphView;

const graphs = ["chw_plant_reset", "ahu_chw_requests", "chw_plant_reset.spark-draft"].map(
  (n) => [n, load(n)] as const,
);

describe("layout", () => {
  it.each(graphs)("%s: places every block and point once", (_, g) => {
    const { nodes, edges } = layout(g);
    expect(nodes).toHaveLength(g.inputs.length + g.blocks.length + g.outputs.length);
    expect(new Set(nodes.map((n) => n.key)).size).toBe(nodes.length);
    expect(edges).toHaveLength(g.connections.length);
  });

  it.each(graphs)("%s: inputs left, outputs right, forward edges run left to right", (_, g) => {
    const { nodes, edges } = layout(g);
    const by = new Map(nodes.map((n) => [n.key, n]));
    const last = Math.max(...nodes.map((n) => n.layer));
    for (const n of nodes) {
      if (n.kind === "input") expect(n.layer).toBe(0);
      if (n.kind === "output") expect(n.layer).toBe(last);
      if (n.kind === "block") expect(n.layer).toBeGreaterThan(0);
    }
    for (const e of edges.filter((x) => !x.back)) {
      expect(by.get(e.target)?.layer ?? 0).toBeGreaterThan(by.get(e.source)?.layer ?? 0);
    }
  });

  it.each(graphs)("%s: nothing overlaps within a column", (_, g) => {
    const { nodes } = layout(g);
    const columns = new Map<number, typeof nodes>();
    for (const n of nodes) columns.set(n.x, [...(columns.get(n.x) ?? []), n]);
    for (const col of columns.values()) {
      const sorted = [...col].sort((a, b) => a.y - b.y);
      sorted.slice(1).forEach((n, i) => {
        const above = sorted[i];
        expect(n.y).toBeGreaterThanOrEqual((above?.y ?? 0) + (above?.height ?? 0));
      });
    }
  });

  it("finds the plant reset's feedback loop and keeps it out of the layering", () => {
    const { edges } = layout(load("chw_plant_reset"));
    expect(edges.filter((e) => e.back).length).toBeGreaterThan(0);
  });

  it("is deterministic", () => {
    const g = load("chw_plant_reset");
    expect(layout(g)).toEqual(layout(structuredClone(g)));
  });

  it("terminates on a pure cycle and ignores edges to unknown nodes", () => {
    const g: GraphView = {
      name: "loop",
      inputs: [],
      outputs: [],
      blocks: [
        { id: "a", class: "Reals.Add", inputs: [{ name: "u1", signal: "Real" }], outputs: [{ name: "y", signal: "Real" }], params: [] },
        { id: "b", class: "Reals.Add", inputs: [{ name: "u1", signal: "Real" }], outputs: [{ name: "y", signal: "Real" }], params: [] },
      ],
      connections: [
        { from: { node: "a", port: "y" }, to: { node: "b", port: "u1" }, signal: "Real" },
        { from: { node: "b", port: "y" }, to: { node: "a", port: "u1" }, signal: "Real" },
        { from: { node: "ghost", port: "y" }, to: { node: "a", port: "u1" }, signal: "Real" },
      ],
    };
    const { nodes, edges } = layout(g);
    expect(nodes).toHaveLength(2);
    expect(edges).toHaveLength(2);
    expect(edges.filter((e) => e.back)).toHaveLength(1);
  });
});
