import { Building2, CircleAlert, CircleCheck, History, LoaderCircle, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useResource } from "@/hooks/useResource";
import { useConfirmBinding, useProposeBinding } from "@/hooks/useWorkbench";
import type { Binding } from "@/types/generated/Binding";
import type { BindingView } from "@/types/generated/BindingView";

import { ErrorNote } from "../ErrorNote";

const KINDS = ["measurement", "above", "constant"] as const;

function num(text: string, fallback: number): number {
  const n = Number(text);
  return text.trim() !== "" && Number.isFinite(n) ? n : fallback;
}

/** The binding as a table; editable when `onChange` is given. */
function BindingTable({ binding, onChange }: { binding: Binding; onChange?: (b: Binding) => void }) {
  const edit = onChange !== undefined;
  const setInput = (i: number, patch: Partial<Binding["inputs"][number]>) =>
    onChange?.({ ...binding, inputs: binding.inputs.map((b, j) => (j === i ? { ...b, ...patch } : b)) });
  const setOutput = (i: number, patch: Partial<Binding["outputs"][number]>) =>
    onChange?.({ ...binding, outputs: binding.outputs.map((b, j) => (j === i ? { ...b, ...patch } : b)) });
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm">
        Runs on floor{binding.floors.length === 1 ? "" : "s"} {binding.floors.join(", ")}.{" "}
        <span className="text-muted-foreground">
          <code>{"{floor}"}</code> in a point name stands for each floor's own point.
        </span>
      </p>
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Input</TableHead>
              <TableHead>Fed by</TableHead>
              <TableHead>BOPTEST point</TableHead>
              <TableHead>Threshold or value</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {binding.inputs.map((b, i) => (
              <TableRow key={b.input}>
                <TableCell className="font-mono text-xs">{b.input}</TableCell>
                <TableCell>
                  {edit ? (
                    <select
                      aria-label={`How ${b.input} is fed`}
                      value={b.kind}
                      onChange={(e) => setInput(i, { kind: e.target.value })}
                      className="h-7 rounded-md border bg-transparent px-1 text-sm"
                    >
                      {KINDS.map((k) => (
                        <option key={k} value={k}>
                          {k}
                        </option>
                      ))}
                    </select>
                  ) : (
                    b.kind
                  )}
                </TableCell>
                <TableCell className="font-mono text-xs">
                  {edit && b.kind !== "constant" ? (
                    <Input aria-label={`Point for ${b.input}`} value={b.point} onChange={(e) => setInput(i, { point: e.target.value })} className="h-7 font-mono text-xs" />
                  ) : (
                    b.point || "—"
                  )}
                </TableCell>
                <TableCell>
                  {b.kind === "above" &&
                    (edit ? (
                      <Input aria-label={`Threshold for ${b.input}`} type="number" step="any" defaultValue={b.threshold} onBlur={(e) => setInput(i, { threshold: num(e.target.value, b.threshold) })} className="h-7 w-24" />
                    ) : (
                      `true above ${b.threshold}`
                    ))}
                  {b.kind === "constant" && String(b.value)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Output</TableHead>
              <TableHead>Writes</TableHead>
              <TableHead>Limits</TableHead>
              <TableHead>Shown beside</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {binding.outputs.map((b, i) => (
              <TableRow key={b.output}>
                <TableCell className="font-mono text-xs">{b.output}</TableCell>
                <TableCell className="font-mono text-xs">
                  {edit ? (
                    <Input aria-label={`Point ${b.output} writes`} value={b.point} placeholder="not sent" onChange={(e) => setOutput(i, { point: e.target.value })} className="h-7 font-mono text-xs" />
                  ) : (
                    b.point || "not sent (shown only)"
                  )}
                </TableCell>
                <TableCell className="whitespace-nowrap">
                  {b.point === "" ? (
                    "—"
                  ) : edit ? (
                    <span className="flex gap-1">
                      <Input aria-label={`Lowest ${b.output}`} type="number" step="any" defaultValue={b.min} onBlur={(e) => setOutput(i, { min: num(e.target.value, b.min) })} className="h-7 w-24" />
                      <Input aria-label={`Highest ${b.output}`} type="number" step="any" defaultValue={b.max} onBlur={(e) => setOutput(i, { max: num(e.target.value, b.max) })} className="h-7 w-24" />
                    </span>
                  ) : (
                    `${b.min} to ${b.max}`
                  )}
                </TableCell>
                <TableCell className="font-mono text-xs">{b.measured || "—"}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}

/**
 * Connects an engineer's sequence to the emulated building: Muse Spark proposes which BOPTEST
 * points feed each input and which setpoint each output writes, the checks list anything that
 * cannot run, and the engineer edits and confirms. Only then can a run use the sequence.
 */
export function BindingPanel({ spec, onBound }: { spec: string; onBound: () => void }) {
  const current = useResource<BindingView>(`/api/workbench/${encodeURIComponent(spec)}/binding`);
  const propose = useProposeBinding(spec);
  const confirm = useConfirmBinding(spec);
  const [draft, setDraft] = useState<Binding | null>(null);
  const proposal = propose.state.status === "done" ? propose.state.data : null;
  useEffect(() => {
    if (proposal) setDraft(proposal.binding);
  }, [proposal]);
  useEffect(() => {
    if (confirm.state.status === "done") {
      setDraft(null);
      current.reload();
      onBound();
    }
  }, [confirm.state, current.reload, onBound]);

  const bound = current.status === "ready" ? current.data : null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Building2 aria-hidden="true" className="size-4" /> Run it on the building
        </CardTitle>
        <CardDescription className="max-w-prose">
          To run on BOPTEST's office building, each input needs a reading and each output a setpoint to write, within
          limits. Muse Spark proposes them from the building's point list; the server checks every point exists and
          fits; you confirm. A run replays your scenarios against the draft again before it writes anything.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {bound && !draft && (
          <>
            <Badge variant="secondary" className="self-start">
              <CircleCheck aria-hidden="true" /> Bound: choose a verified draft, then run it from the Runs tab
            </Badge>
            <BindingTable binding={bound.binding} />
          </>
        )}
        {!draft && (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant={bound ? "outline" : "default"} onClick={() => propose.run(undefined)} disabled={propose.state.status === "running"}>
              {propose.state.status === "running" ? (
                <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" />
              ) : (
                <Sparkles aria-hidden="true" />
              )}
              {bound ? "Propose again" : "Propose a binding with Muse Spark"}
            </Button>
          </div>
        )}
        {propose.state.status === "error" && <ErrorNote error={propose.state.error} />}
        {draft && proposal && (
          <>
            <Badge variant="outline" className="self-start">
              {proposal.proposed_by === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
              Proposed by Muse Spark{proposal.proposed_by === "recorded" ? " (recorded)" : ""}; edit before you confirm
            </Badge>
            {proposal.binding.notes.length > 0 && (
              <ul className="list-disc pl-5 text-sm text-muted-foreground">
                {proposal.binding.notes.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            )}
            {proposal.problems.length > 0 && (
              <Alert variant="destructive">
                <CircleAlert aria-hidden="true" />
                <AlertTitle>The checks refuse this binding as proposed</AlertTitle>
                <AlertDescription>
                  <ul className="list-disc pl-4">
                    {proposal.problems.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            )}
            <BindingTable binding={draft} onChange={setDraft} />
            {confirm.state.status === "error" && <ErrorNote error={confirm.state.error} />}
            <div className="flex flex-wrap gap-2">
              <Button onClick={() => confirm.run({ binding: draft })} disabled={confirm.state.status === "running"}>
                <CircleCheck aria-hidden="true" /> Confirm this binding
              </Button>
              <Button variant="ghost" onClick={() => setDraft(null)}>
                Cancel
              </Button>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
