/**
 * A small layered layout for block graphs: boundary inputs on the left, outputs on the right,
 * blocks in columns by their longest path from the inputs, and each column ordered to reduce
 * crossings. Pure and deterministic, so the same graph always lands in the same place.
 *
 * Control graphs loop back through delay blocks; those back edges are found by a depth-first
 * search (visiting in a fixed order) and ignored for layering, so a loop never stretches the
 * picture or hangs the layout.
 */
import type { GraphView } from "@/types/generated/GraphView";
import type { Signal } from "@/types/generated/Signal";

export type NodeKind = "input" | "block" | "output";

export interface LaidNode {
  /** `in:<name>`, `out:<name>` or `b:<block id>`, so a block and a point may share a name. */
  key: string;
  kind: NodeKind;
  /** The block id or boundary name, as in the graph (the CXF local name). */
  name: string;
  layer: number;
  x: number;
  y: number;
  height: number;
}

export interface LaidEdge {
  key: string;
  source: string;
  sourcePort: string | null;
  target: string;
  targetPort: string | null;
  signal: Signal;
  /** True for an edge that closes a loop (drawn, but not used for layering). */
  back: boolean;
}

export const COLUMN = 260;
export const ROW_GAP = 28;
const PORT_ROW = 22;
const HEADER = 44;

export function nodeHeight(kind: NodeKind, ports: number): number {
  return kind === "block" ? HEADER + Math.max(1, ports) * PORT_ROW : 40;
}

export function layout(graph: GraphView): { nodes: LaidNode[]; edges: LaidEdge[] } {
  const keyOf = (node: string, port: string | undefined, side: "from" | "to") =>
    port === undefined ? `${side === "from" ? "in" : "out"}:${node}` : `b:${node}`;
  const nodes = new Map<string, LaidNode>();
  const add = (key: string, kind: NodeKind, name: string, ports: number) =>
    nodes.set(key, { key, kind, name, layer: 0, x: 0, y: 0, height: nodeHeight(kind, ports) });
  for (const p of graph.inputs) add(`in:${p.name}`, "input", p.name, 0);
  for (const b of graph.blocks) add(`b:${b.id}`, "block", b.id, Math.max(b.inputs.length, b.outputs.length));
  for (const p of graph.outputs) add(`out:${p.name}`, "output", p.name, 0);

  const edges: LaidEdge[] = graph.connections
    .map((c, i) => ({
      key: `e${i}`,
      source: keyOf(c.from.node, c.from.port, "from"),
      sourcePort: c.from.port ?? null,
      target: keyOf(c.to.node, c.to.port, "to"),
      targetPort: c.to.port ?? null,
      signal: c.signal,
      back: false,
    }))
    .filter((e) => nodes.has(e.source) && nodes.has(e.target));

  // Back edges: DFS from inputs first, then any unvisited node, in declaration order.
  const out = new Map<string, LaidEdge[]>();
  for (const e of edges) out.set(e.source, [...(out.get(e.source) ?? []), e]);
  const state = new Map<string, "open" | "done">();
  const visit = (start: string) => {
    const stack: { key: string; next: number }[] = [{ key: start, next: 0 }];
    state.set(start, "open");
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      if (!top) break;
      const list = out.get(top.key) ?? [];
      const e = list[top.next];
      if (e === undefined) {
        state.set(top.key, "done");
        stack.pop();
        continue;
      }
      top.next += 1;
      const s = state.get(e.target);
      if (s === "open") e.back = true;
      else if (s === undefined) {
        state.set(e.target, "open");
        stack.push({ key: e.target, next: 0 });
      }
    }
  };
  for (const key of nodes.keys()) if (!state.has(key)) visit(key);

  // Longest path over forward edges (Kahn order, so it terminates on any input).
  const forward = edges.filter((e) => !e.back);
  const indeg = new Map<string, number>([...nodes.keys()].map((k) => [k, 0]));
  for (const e of forward) indeg.set(e.target, (indeg.get(e.target) ?? 0) + 1);
  const queue = [...nodes.keys()].filter((k) => indeg.get(k) === 0);
  for (let i = 0; i < queue.length; i += 1) {
    const k = queue[i];
    if (k === undefined) break;
    const n = nodes.get(k);
    for (const e of forward.filter((f) => f.source === k)) {
      const t = nodes.get(e.target);
      if (n && t) t.layer = Math.max(t.layer, n.layer + 1);
      indeg.set(e.target, (indeg.get(e.target) ?? 1) - 1);
      if (indeg.get(e.target) === 0) queue.push(e.target);
    }
  }
  // A block that reads nothing (a constant, a clock) sits just before its first reader rather
  // than piling up in the first column.
  for (const n of nodes.values()) {
    if (n.kind !== "block" || forward.some((e) => e.target === n.key)) continue;
    const readers = forward
      .filter((e) => e.source === n.key)
      .map((e) => nodes.get(e.target)?.layer ?? 1);
    if (readers.length > 0) n.layer = Math.max(1, Math.min(...readers) - 1);
  }
  // Inputs in the first column, outputs in the last.
  let last = 0;
  for (const n of nodes.values()) {
    if (n.kind === "input") n.layer = 0;
    else if (n.kind === "block") n.layer = Math.max(1, n.layer);
    if (n.kind !== "output") last = Math.max(last, n.layer);
  }
  for (const n of nodes.values()) if (n.kind === "output") n.layer = last + 1;

  // Order each column by the mean position of its neighbours, a few sweeps each way.
  const columns: LaidNode[][] = [];
  for (const n of nodes.values()) (columns[n.layer] ??= []).push(n);
  const index = new Map<string, number>();
  const reindex = () => columns.forEach((col) => col?.forEach((n, i) => index.set(n.key, i)));
  reindex();
  const mean = (keys: string[]) =>
    keys.length === 0 ? null : keys.reduce((s, k) => s + (index.get(k) ?? 0), 0) / keys.length;
  for (let sweep = 0; sweep < 4; sweep += 1) {
    const down = sweep % 2 === 0;
    const order = down ? columns.keys() : [...columns.keys()].reverse();
    for (const l of order) {
      const col = columns[l];
      if (!col) continue;
      const score = new Map(
        col.map((n) => {
          const near = forward
            .filter((e) => (down ? e.target === n.key : e.source === n.key))
            .map((e) => (down ? e.source : e.target));
          return [n.key, mean(near) ?? index.get(n.key) ?? 0] as const;
        }),
      );
      col.sort((a, b) => (score.get(a.key) ?? 0) - (score.get(b.key) ?? 0) || a.key.localeCompare(b.key));
      reindex();
    }
  }

  // Coordinates: columns left to right, nodes stacked and centred on the tallest column.
  const heights = columns.map((col) => (col ?? []).reduce((h, n) => h + n.height + ROW_GAP, -ROW_GAP));
  const tallest = Math.max(0, ...heights);
  columns.forEach((col, l) => {
    let y = (tallest - (heights[l] ?? 0)) / 2;
    for (const n of col ?? []) {
      n.x = l * COLUMN;
      n.y = y;
      y += n.height + ROW_GAP;
    }
  });
  return { nodes: [...nodes.values()], edges };
}
