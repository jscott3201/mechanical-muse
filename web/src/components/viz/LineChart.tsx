import { useId, useMemo, useRef, useState } from "react";

import { useWidth } from "./useWidth";

export interface Series {
  id: string;
  label: string;
  /** A CSS color, normally var(--viz-N). */
  color: string;
  dashed?: boolean;
  /** y is null where there is no data: the line breaks instead of drawing a value. */
  points: { x: number; y: number | null }[];
}

interface Props {
  title: string;
  unit: string;
  series: Series[];
  formatX: (x: number) => string;
  formatY: (y: number) => string;
  /** Draw horizontal steps (counts, latched states) instead of slopes. */
  step?: boolean;
  height?: number;
}

const PAD = { top: 12, right: 76, bottom: 24, left: 48 };

function niceTicks(lo: number, hi: number, count = 4): number[] {
  if (!(hi > lo)) return [lo];
  const raw = (hi - lo) / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + step * 1e-9; v += step) out.push(v);
  return out;
}

/**
 * One measure on one y-scale against BOPTEST simulation time. Missing values break the line.
 * Identity is never colour alone: a legend, labels at the line ends, and a table view.
 */
export function LineChart({ title, unit, series, formatX, formatY, step = false, height = 168 }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const [hover, setHover] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const titleId = useId();

  const { xs, x0, x1, y0, y1 } = useMemo(() => {
    const xsSet = new Set<number>();
    let ylo = Infinity;
    let yhi = -Infinity;
    for (const s of series)
      for (const p of s.points) {
        xsSet.add(p.x);
        if (p.y !== null) {
          ylo = Math.min(ylo, p.y);
          yhi = Math.max(yhi, p.y);
        }
      }
    const xsSorted = [...xsSet].sort((a, b) => a - b);
    if (!Number.isFinite(ylo)) {
      ylo = 0;
      yhi = 1;
    }
    const span = yhi - ylo || Math.abs(yhi) * 0.1 || 1;
    return {
      xs: xsSorted,
      x0: xsSorted[0] ?? 0,
      x1: xsSorted.at(-1) ?? 1,
      y0: ylo - span * 0.08,
      y1: yhi + span * 0.08,
    };
  }, [series]);

  const plotW = Math.max(width - PAD.left - PAD.right, 10);
  const plotH = height - PAD.top - PAD.bottom;
  const sx = (x: number) => PAD.left + (x1 > x0 ? ((x - x0) / (x1 - x0)) * plotW : plotW / 2);
  const sy = (y: number) => PAD.top + plotH - ((y - y0) / (y1 - y0)) * plotH;

  const path = (s: Series) => {
    let d = "";
    let prev: { x: number; y: number } | null = null;
    for (const p of s.points) {
      if (p.y === null) {
        prev = null;
        continue;
      }
      const X = sx(p.x);
      const Y = sy(p.y);
      if (prev === null) d += `M${X},${Y}`;
      else d += step ? `H${X}V${Y}` : `L${X},${Y}`;
      prev = { x: X, y: Y };
    }
    return d;
  };

  const hoverX = hover === null ? null : xs[hover];
  const lastOf = (s: Series) => [...s.points].reverse().find((p) => p.y !== null);
  const valueAt = (s: Series, x: number) => s.points.find((p) => p.x === x)?.y ?? null;

  const onMove = (e: React.PointerEvent<SVGRectElement>) => {
    if (xs.length === 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const x = x0 + ((px / rect.width) * (x1 - x0) || 0);
    let best = 0;
    for (let i = 1; i < xs.length; i++) {
      if (Math.abs((xs[i] ?? 0) - x) < Math.abs((xs[best] ?? 0) - x)) best = i;
    }
    setHover(best);
  };

  return (
    <figure className="flex min-w-0 flex-col gap-1" aria-labelledby={titleId}>
      <figcaption className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-sm">
        <span id={titleId} className="font-medium">
          {title} <span className="text-muted-foreground">({unit})</span>
        </span>
        {series.length > 1 && (
          <span className="flex flex-wrap gap-3 text-xs text-muted-foreground">
            {series.map((s) => (
              <span key={s.id} className="inline-flex items-center gap-1">
                <svg width="16" height="6" aria-hidden="true">
                  <line x1="0" x2="16" y1="3" y2="3" stroke={s.color} strokeWidth="2" strokeDasharray={s.dashed ? "4 3" : undefined} />
                </svg>
                {s.label}
              </span>
            ))}
          </span>
        )}
        <button type="button" className="ml-auto text-xs text-muted-foreground underline-offset-2 hover:underline" onClick={() => setTable((t) => !t)}>
          {table ? "Show chart" : "Show table"}
        </button>
      </figcaption>
      {table ? (
        <div className="max-h-64 overflow-auto rounded border">
          <table className="w-full text-xs tabular-nums">
            <thead className="sticky top-0 bg-background">
              <tr>
                <th className="px-2 py-1 text-left font-medium">Time</th>
                {series.map((s) => (
                  <th key={s.id} className="px-2 py-1 text-right font-medium">{s.label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {xs.map((x) => (
                <tr key={x} className="border-t">
                  <td className="px-2 py-0.5">{formatX(x)}</td>
                  {series.map((s) => {
                    const v = valueAt(s, x);
                    return (
                      <td key={s.id} className="px-2 py-0.5 text-right">{v === null ? "no data" : formatY(v)}</td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div ref={ref} className="relative w-full">
          <svg width="100%" height={height} viewBox={`0 0 ${Math.max(width, 1)} ${height}`} role="img" aria-label={`${title}, ${unit}`}>
            {niceTicks(y0, y1).map((t) => (
              <g key={t}>
                <line x1={PAD.left} x2={PAD.left + plotW} y1={sy(t)} y2={sy(t)} className="stroke-border" strokeWidth="1" />
                <text x={PAD.left - 6} y={sy(t)} dy="0.32em" textAnchor="end" className="fill-muted-foreground text-[10px]">{formatY(t)}</text>
              </g>
            ))}
            {[x0, (x0 + x1) / 2, x1].map((t, i) => (
              <text key={i} x={sx(t)} y={height - 6} textAnchor={i === 0 ? "start" : i === 2 ? "end" : "middle"} className="fill-muted-foreground text-[10px]">{formatX(t)}</text>
            ))}
            {series.map((s) => (
              <path key={s.id} d={path(s)} fill="none" stroke={s.color} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" strokeDasharray={s.dashed ? "6 4" : undefined} />
            ))}
            {series.map((s, i) => {
              const last = lastOf(s);
              if (!last || last.y === null) return null;
              return (
                <text key={s.id} x={sx(last.x) + 6} y={sy(last.y) + (i - (series.length - 1) / 2) * 11} dy="0.32em" className="fill-foreground text-[10px]">{s.label}</text>
              );
            })}
            {hoverX !== null && hoverX !== undefined && (
              <g>
                <line x1={sx(hoverX)} x2={sx(hoverX)} y1={PAD.top} y2={PAD.top + plotH} className="stroke-muted-foreground" strokeWidth="1" />
                {series.map((s) => {
                  const v = valueAt(s, hoverX);
                  return v === null ? null : <circle key={s.id} cx={sx(hoverX)} cy={sy(v)} r="4" fill={s.color} className="stroke-background" strokeWidth="2" />;
                })}
              </g>
            )}
            <rect x={PAD.left} y={PAD.top} width={plotW} height={plotH} fill="transparent" onPointerMove={onMove} onPointerLeave={() => setHover(null)} />
          </svg>
          {hoverX !== null && hoverX !== undefined && (
            <div
              role="status"
              className="pointer-events-none absolute top-1 rounded border bg-popover px-2 py-1 text-xs text-popover-foreground shadow-sm"
              style={sx(hoverX) > width / 2 ? { right: width - sx(hoverX) + 8 } : { left: sx(hoverX) + 8 }}
            >
              <div className="font-medium">{formatX(hoverX)}</div>
              {series.map((s) => {
                const v = valueAt(s, hoverX);
                return (
                  <div key={s.id} className="tabular-nums">
                    {s.label}: {v === null ? "no data" : `${formatY(v)} ${unit}`}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </figure>
  );
}
