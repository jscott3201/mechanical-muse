import { clock, scenarioEnd } from "@/lib/workbench";
import { formatWithUnit } from "@/lib/format";
import type { PointDraft } from "@/types/generated/PointDraft";
import type { ScenarioDraft } from "@/types/generated/ScenarioDraft";

const W = 640;
const ROW = 30;
const LABEL = 150;
const PAD = 8;
const TOP = 10;

function show(value: number | boolean, unit: string): string {
  return typeof value === "boolean" ? (value ? "on" : "off") : formatWithUnit(value, unit);
}

/**
 * One scenario at a glance: each input's steps over time, then each checked output's windows
 * with the value the scenario expects there. A picture for review, not a plot of results.
 */
export function ScenarioTimeline({
  scenario,
  inputs,
  outputs,
  horizon,
}: {
  scenario: ScenarioDraft;
  inputs: PointDraft[];
  outputs: PointDraft[];
  horizon: number;
}) {
  const end = Math.max(scenarioEnd(scenario, horizon), 1);
  const x = (t: number) => LABEL + ((W - LABEL - PAD) * Math.min(t, end)) / end;
  const checked = outputs.filter((o) => scenario.expect.some((e) => e.output === o.name));
  const rows = inputs.length + checked.length;
  const height = rows * ROW + 22 + TOP;
  const unitOf = (name: string) => [...inputs, ...outputs].find((p) => p.name === name)?.unit ?? "";
  return (
    <svg
      viewBox={`0 -${TOP} ${W} ${height}`}
      width="100%"
      role="img"
      aria-label={`Timeline of scenario ${scenario.name}`}
      className="max-w-[760px] text-[10px]"
    >
      {inputs.map((p, row) => {
        const steps = scenario.inputs.find((i) => i.point === p.name)?.steps ?? [];
        const y = row * ROW;
        const numbers = steps.map((s) => (typeof s.value === "number" ? s.value : s.value ? 1 : 0));
        const lo = Math.min(...numbers);
        const hi = Math.max(...numbers);
        const yv = (v: number) => (hi === lo ? y + ROW / 2 : y + ROW - 6 - ((ROW - 12) * (v - lo)) / (hi - lo));
        return (
          <g key={`in-${p.name}`}>
            <text x={0} y={y + ROW / 2} dy="0.32em" className="fill-muted-foreground font-mono">
              {p.name}
            </text>
            {steps.map((s, i) => {
              const next = steps[i + 1]?.t ?? end;
              const v = numbers[i] ?? 0;
              return (
                <g key={i}>
                  <line x1={x(s.t)} x2={x(next)} y1={yv(v)} y2={yv(v)} stroke="var(--viz-1)" strokeWidth={2} />
                  {i > 0 && (
                    <line x1={x(s.t)} x2={x(s.t)} y1={yv(numbers[i - 1] ?? v)} y2={yv(v)} stroke="var(--viz-1)" strokeWidth={1} />
                  )}
                  {(i === 0 || v !== numbers[i - 1]) && (
                    <text x={x(s.t) + 3} y={yv(v) - 3} className="fill-foreground">
                      {show(s.value as number | boolean, p.unit)}
                    </text>
                  )}
                </g>
              );
            })}
          </g>
        );
      })}
      {checked.map((p, k) => {
        const y = (inputs.length + k) * ROW;
        return (
          <g key={`out-${p.name}`}>
            <text x={0} y={y + ROW / 2} dy="0.32em" className="fill-foreground font-mono font-medium">
              {p.name}
            </text>
            {scenario.expect
              .filter((e) => e.output === p.name)
              .map((e, i) => (
                <g key={i}>
                  <rect
                    x={x(e.from_s)}
                    width={Math.max(x(e.to_s) - x(e.from_s), 2)}
                    y={y + 5}
                    height={ROW - 10}
                    rx={3}
                    fill="var(--viz-2)"
                    fillOpacity={0.18}
                    stroke="var(--viz-2)"
                  />
                  <text x={x(e.from_s) + 4} y={y + ROW / 2} dy="0.32em" className="fill-foreground">
                    {show(e.equals as number | boolean, unitOf(e.output))}
                  </text>
                </g>
              ))}
          </g>
        );
      })}
      <line x1={LABEL} x2={W - PAD} y1={rows * ROW + 4} y2={rows * ROW + 4} className="stroke-border" />
      {[0, 0.25, 0.5, 0.75, 1].map((f) => (
        <text key={f} x={x(f * end)} y={rows * ROW + 16} textAnchor={f === 1 ? "end" : f === 0 ? "start" : "middle"} className="fill-muted-foreground">
          {clock(f * end)}
        </text>
      ))}
    </svg>
  );
}
