import "@xyflow/react/dist/style.css";

import {
  Background,
  Controls,
  type Edge,
  Handle,
  type Node,
  type NodeProps,
  Position,
  ReactFlow,
} from "@xyflow/react";
import { TriangleAlert } from "lucide-react";
import { memo, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";
import type { BlockView } from "@/types/generated/BlockView";
import type { GraphView } from "@/types/generated/GraphView";
import type { Signal } from "@/types/generated/Signal";

import { layout, type NodeKind } from "./layout";

/** Each signal type has a color and a line style, so the type never rests on color alone. */
const SIGNALS: Record<Signal, { color: string; dash: string | undefined; label: string }> = {
  Real: { color: "var(--viz-1)", dash: undefined, label: "Real" },
  Integer: { color: "var(--viz-2)", dash: "6 3", label: "Integer" },
  Boolean: { color: "var(--viz-3)", dash: "2 3", label: "Boolean" },
};

const HEADER = 44;
/** Smallest zoom the initial fit may use: block ids and ports stay legible. */
const MIN_ZOOM = 0.8;
const PORT_ROW = 22;
const portTop = (i: number) => HEADER + i * PORT_ROW + PORT_ROW / 2;

type BlockData = { block: BlockView; flagged: boolean };
type PointData = { kind: NodeKind; name: string; signal: Signal; flagged: boolean };

const BlockNode = memo(function BlockNode({ data }: NodeProps<Node<BlockData>>) {
  const { block, flagged } = data;
  const short = block.class.split(".").at(-1) ?? block.class;
  return (
    <div
      aria-label={`${block.id}, ${block.class}${flagged ? ", has a problem" : ""}`}
      className={cn(
        "relative w-[200px] rounded-md border bg-card text-card-foreground shadow-sm",
        flagged && "ring-2 ring-destructive",
      )}
      style={{ height: HEADER + Math.max(1, block.inputs.length, block.outputs.length) * PORT_ROW }}
      title={block.params.map((p) => `${p.name} = ${JSON.stringify(p.value)}`).join("\n") || undefined}
    >
      <div className="flex items-center gap-1 border-b px-2 py-1">
        {flagged && <TriangleAlert aria-hidden="true" className="size-3 shrink-0 text-destructive" />}
        <div className="min-w-0">
          <p className="truncate font-mono text-xs font-medium">{block.id}</p>
          <p className="truncate text-[10px] text-muted-foreground">{short}</p>
        </div>
      </div>
      {block.inputs.map((p, i) => (
        <div key={`in-${p.name}`}>
          <Handle
            type="target"
            position={Position.Left}
            id={p.name}
            style={{ top: portTop(i), background: SIGNALS[p.signal].color }}
          />
          <span className="absolute left-2 font-mono text-[10px]" style={{ top: portTop(i) - 7 }}>
            {p.name}
          </span>
        </div>
      ))}
      {block.outputs.map((p, i) => (
        <div key={`out-${p.name}`}>
          <Handle
            type="source"
            position={Position.Right}
            id={p.name}
            style={{ top: portTop(i), background: SIGNALS[p.signal].color }}
          />
          <span className="absolute right-2 font-mono text-[10px]" style={{ top: portTop(i) - 7 }}>
            {p.name}
          </span>
        </div>
      ))}
    </div>
  );
});

const PointNode = memo(function PointNode({ data }: NodeProps<Node<PointData>>) {
  const input = data.kind === "input";
  return (
    <div
      aria-label={`${input ? "input" : "output"} ${data.name}`}
      className={cn(
        "flex h-10 w-[200px] items-center rounded-full border bg-muted px-3 font-mono text-xs",
        input ? "justify-start" : "justify-end",
        data.flagged && "ring-2 ring-destructive",
      )}
    >
      <span className="truncate">{data.name}</span>
      <Handle
        type={input ? "source" : "target"}
        position={input ? Position.Right : Position.Left}
        style={{ background: SIGNALS[data.signal].color }}
      />
    </div>
  );
});

const nodeTypes = { block: BlockNode, point: PointNode };

/** The app's theme (shadcn's `.dark` class), not the OS one, so the canvas matches the page. */
function appTheme(): "light" | "dark" {
  return typeof document !== "undefined" && document.documentElement.classList.contains("dark")
    ? "dark"
    : "light";
}

function Legend() {
  return (
    <ul aria-label="Signal types" className="flex flex-wrap gap-3 text-xs text-muted-foreground">
      {(Object.keys(SIGNALS) as Signal[]).map((s) => (
        <li key={s} className="flex items-center gap-1">
          <svg width="28" height="8" aria-hidden="true">
            <line x1="0" y1="4" x2="28" y2="4" stroke={SIGNALS[s].color} strokeWidth="2" strokeDasharray={SIGNALS[s].dash} />
          </svg>
          {SIGNALS[s].label}
        </li>
      ))}
      <li className="flex items-center gap-1">
        <svg width="28" height="8" aria-hidden="true">
          <line x1="0" y1="4" x2="28" y2="4" stroke="currentColor" strokeWidth="2" strokeOpacity="0.4" />
        </svg>
        loop back (through a delay)
      </li>
    </ul>
  );
}

function ConnectionTable({ graph }: { graph: GraphView }) {
  const end = (e: { node: string; port?: string }) => (e.port ? `${e.node}.${e.port}` : e.node);
  return (
    <div className="max-h-[480px] overflow-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>From</TableHead>
            <TableHead>To</TableHead>
            <TableHead>Type</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {graph.connections.map((c, i) => (
            <TableRow key={i}>
              <TableCell className="font-mono text-xs">{end(c.from)}</TableCell>
              <TableCell className="font-mono text-xs">{end(c.to)}</TableCell>
              <TableCell>{c.signal}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

/**
 * A read-only block graph: inputs left, outputs right, blocks in layers. Blocks or points
 * named in `flagged` are outlined (a diagnostic's subject). A table of connections is one
 * click away for anyone who would rather read than look.
 */
export function GraphCanvas({
  graph,
  flagged = [],
  label,
}: {
  graph: GraphView;
  flagged?: string[];
  label: string;
}) {
  const [asTable, setAsTable] = useState(false);
  const { nodes, edges, height } = useMemo(() => {
    const laid = layout(graph);
    // The canvas is as tall as the graph at a readable zoom, within limits; a long chain is
    // panned sideways rather than shrunk until its labels cannot be read.
    const bottom = Math.max(0, ...laid.nodes.map((n) => n.y + n.height));
    const height = Math.min(640, Math.max(300, bottom * MIN_ZOOM + 96));
    const marks = new Set(flagged);
    const blocks = new Map(graph.blocks.map((b) => [b.id, b]));
    const points = new Map([...graph.inputs, ...graph.outputs].map((p) => [p.name, p]));
    const nodes: Node[] = laid.nodes.map((n) => {
      const block = n.kind === "block" ? blocks.get(n.name) : undefined;
      return block
        ? { id: n.key, type: "block", position: { x: n.x, y: n.y }, data: { block, flagged: marks.has(n.name) } }
        : {
            id: n.key,
            type: "point",
            position: { x: n.x, y: n.y },
            data: {
              kind: n.kind,
              name: n.name,
              signal: points.get(n.name)?.signal ?? "Real",
              flagged: marks.has(n.name),
            },
          };
    });
    const edges: Edge[] = laid.edges.map((e) => ({
      id: e.key,
      source: e.source,
      sourceHandle: e.sourcePort,
      target: e.target,
      targetHandle: e.targetPort,
      style: {
        stroke: SIGNALS[e.signal].color,
        strokeWidth: 2,
        strokeDasharray: SIGNALS[e.signal].dash,
        strokeOpacity: e.back ? 0.4 : 1,
      },
    }));
    return { nodes, edges, height };
    // `flagged` is compared by content, so a new array with the same names keeps the layout.
  }, [graph, flagged.join(",")]);
  return (
    <section aria-label={label} className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Legend />
        <Button size="sm" variant="outline" onClick={() => setAsTable((t) => !t)}>
          {asTable ? "Show as graph" : "Show as table"}
        </Button>
      </div>
      {asTable ? (
        <ConnectionTable graph={graph} />
      ) : (
        <div className="w-full rounded-md border" style={{ height }}>
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            fitView
            fitViewOptions={{ padding: 0.04, minZoom: MIN_ZOOM }}
            onInit={(flow) => {
              // A graph wider than the canvas at a legible zoom starts at its inputs (left).
              void flow.fitView({ padding: 0.04, minZoom: MIN_ZOOM }).then(() => {
                const v = flow.getViewport();
                if (v.x < 0) void flow.setViewport({ ...v, x: 16 });
              });
            }}
            minZoom={0.2}
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable={false}
            colorMode={appTheme()}
            proOptions={{ hideAttribution: false }}
            ariaLabelConfig={{ "node.a11yDescription.default": label }}
          >
            <Background />
            <Controls showInteractive={false} />
          </ReactFlow>
        </div>
      )}
    </section>
  );
}
