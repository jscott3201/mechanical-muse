import { CircleAlert, FileUp, History, LoaderCircle, MessageCircleQuestion, RotateCcw, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useResource } from "@/hooks/useResource";
import { useAcceptSequence, useReadSequence } from "@/hooks/useWorkbench";
import { formatWithUnit } from "@/lib/format";
import {
  ACCEPTED_TYPES,
  acceptedDraft,
  clock,
  fileProblem,
  fileToAttachment,
  totalProblem,
  withExpected,
} from "@/lib/workbench";
import type { BindingView } from "@/types/generated/BindingView";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { PointDraft } from "@/types/generated/PointDraft";
import type { ReadView } from "@/types/generated/ReadView";
import type { SequenceDraft } from "@/types/generated/SequenceDraft";
import type { WorkbenchAttachment } from "@/types/generated/WorkbenchAttachment";

import { ErrorNote } from "../ErrorNote";
import { JourneyBar } from "./JourneyBar";
import { ScenarioTimeline } from "./ScenarioTimeline";

const SAMPLES = [
  {
    url: "/samples/ahu-sat-reset-sample.png",
    name: "ahu-sat-reset-sample.png",
    label: "Use the sample sequence sheet",
    sequence: null,
  },
  {
    url: "/samples/ahu-sat-reset-sample-v2.png",
    name: "ahu-sat-reset-sample-v2.png",
    label: "Revision B of the sample",
    // Kept apart from revision A's sequence, and matching the recorded revision B runs.
    sequence: "ahu1_sat_rev_b",
  },
] as const;

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return <span className="tabular-nums">{Math.max(0, Math.round((now - since) / 1000))} s</span>;
}

/** Step 1: the engineer's document, as text, PDF or image. */
function SourceStep({
  onRead,
}: {
  onRead: (text: string, files: WorkbenchAttachment[], sequence: string | null) => void;
}) {
  const [text, setText] = useState("");
  // A sample may suggest its sequence name; any other input drops the suggestion.
  const [suggested, setSuggested] = useState<string | null>(null);
  const [files, setFiles] = useState<WorkbenchAttachment[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);

  const add = async (list: FileList | File[]) => {
    setProblem(null);
    setSuggested(null);
    const next: WorkbenchAttachment[] = [];
    for (const f of Array.from(list)) {
      const p = fileProblem(f) ?? totalProblem([...files, ...next], f.size);
      if (p) {
        setProblem(p);
        continue;
      }
      next.push(await fileToAttachment(f, f.name));
    }
    setFiles((old) => [...old, ...next].slice(0, 4));
  };
  const loadSample = async (sample: (typeof SAMPLES)[number]) => {
    setProblem(null);
    try {
      const response = await fetch(sample.url);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const blob = await response.blob();
      setFiles([await fileToAttachment(blob, sample.name)]);
    } catch (e: unknown) {
      setProblem(`Could not load the sample (${e instanceof Error ? e.message : "network error"}); attach a file or paste text instead.`);
      return;
    }
    setText("");
    setPreview(sample.url);
    setSuggested(sample.sequence);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">Bring your own sequence</CardTitle>
        <CardDescription className="max-w-prose">
          Give Muse Spark a written sequence of operations: paste the text, or attach a PDF or a photo of the sheet.
          It writes a precise task, lists what the document leaves open, and proposes test scenarios. You review and
          accept those scenarios; they become the gate every draft must pass on the engine.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-col gap-2">
          <Label htmlFor="wb-text">Sequence text</Label>
          <textarea
            id="wb-text"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setSuggested(null);
            }}
            rows={8}
            placeholder="3.1 While the AHU is off, the setpoint is 55 °F. …"
            className="w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
          />
        </div>
        <label
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            void add(e.dataTransfer.files);
          }}
          className="flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground hover:bg-muted/50"
        >
          <FileUp aria-hidden="true" className="size-5" />
          <span>Drop a PDF or image here, or choose files (up to 4, 14 MB in all)</span>
          <input
            type="file"
            multiple
            accept={ACCEPTED_TYPES.join(",")}
            className="sr-only"
            onChange={(e) => e.target.files && void add(e.target.files)}
          />
        </label>
        {problem && <p role="alert" className="text-sm text-destructive">{problem}</p>}
        {files.length > 0 && (
          <ul className="flex flex-wrap gap-2">
            {files.map((f, i) => (
              <li key={`${f.name}-${i}`}>
                <Badge variant="secondary" className="gap-1">
                  {f.name}
                  <button
                    type="button"
                    aria-label={`Remove ${f.name}`}
                    onClick={() => {
                      setFiles((old) => old.filter((_, j) => j !== i));
                      setPreview(null);
                    }}
                  >
                    <X aria-hidden="true" className="size-3" />
                  </button>
                </Badge>
              </li>
            ))}
          </ul>
        )}
        {preview && (
          <img src={preview} alt="The sample sequence sheet" className="max-h-80 w-auto self-start rounded-md border" />
        )}
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => onRead(text, files, suggested)} disabled={!text.trim() && files.length === 0}>
            <Sparkles aria-hidden="true" /> Read with Muse Spark
          </Button>
          {SAMPLES.map((sample) => (
            <Button key={sample.url} variant="outline" onClick={() => void loadSample(sample)}>
              {sample.label}
            </Button>
          ))}
        </div>
        <p className="text-xs text-muted-foreground">
          The sample is a one-page sheet we wrote for this demo (an AHU supply air temperature reset, in °F); revision B
          raises its humidity thresholds to suit this building. Without a model key, the server answers with its recorded
          reading of either, labelled recorded.
        </p>
      </CardContent>
    </Card>
  );
}

function PointList({ title, points }: { title: string; points: PointDraft[] }) {
  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{title}</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Meaning</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {points.map((p) => (
            <TableRow key={p.name}>
              <TableCell className="font-mono text-xs">{p.name}</TableCell>
              <TableCell>{[p.type, p.unit !== "1" && p.unit !== "bool" ? p.unit : null].filter(Boolean).join(" · ")}</TableCell>
              <TableCell className="whitespace-normal">{p.meaning}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function ExpectedInput({
  value,
  unit,
  onChange,
  label,
}: {
  value: number | boolean;
  unit: string;
  onChange: (v: number | boolean) => void;
  label: string;
}) {
  if (typeof value === "boolean") {
    return (
      <Button size="sm" variant="outline" aria-label={label} onClick={() => onChange(!value)}>
        {value ? "true" : "false"}
      </Button>
    );
  }
  return (
    <span className="flex items-center gap-2">
      <Input
        aria-label={label}
        type="number"
        step="any"
        defaultValue={value}
        onBlur={(e) => {
          const text = e.target.value.trim();
          const n = Number(text);
          // An emptied or unreadable field goes back to the value it had, never to 0.
          if (text !== "" && Number.isFinite(n)) onChange(n);
          else e.target.value = String(value);
        }}
        className="h-7 w-32"
      />
      <span className="text-xs text-muted-foreground">{formatWithUnit(value, unit)}</span>
    </span>
  );
}

/** Step 2: the engineer reviews Spark's reading, keeps or edits scenarios, and accepts. */
function ReviewStep({
  view,
  name,
  onAccepted,
  onRestart,
}: {
  view: ReadView;
  /** A sequence name to start from instead of the one Spark proposed (still editable). */
  name: string | null;
  onAccepted: (spec: string) => void;
  onRestart: () => void;
}) {
  const [draft, setDraft] = useState<SequenceDraft>(name ? { ...view.draft, name } : view.draft);
  const [kept, setKept] = useState<Set<string>>(() => new Set(view.draft.scenarios.map((s) => s.name)));
  const accept = useAcceptSequence();
  const unitOf = useMemo(() => {
    const units = new Map([...draft.inputs, ...draft.outputs].map((p) => [p.name, p.unit]));
    return (name: string) => units.get(name) ?? "";
  }, [draft]);
  // Hands over once per accepted sequence: this page stays mounted while hidden, so a new
  // `onAccepted` identity must not hand the same sequence over again.
  const handedOver = useRef<unknown>(null);
  useEffect(() => {
    if (accept.state.status !== "done" || handedOver.current === accept.state.data) return;
    handedOver.current = accept.state.data;
    onAccepted(accept.state.data.spec.summary.name);
  }, [accept.state, onAccepted]);

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0 flex-1">
              <CardTitle className="text-lg">{draft.title}</CardTitle>
              <div className="mt-2 flex flex-wrap gap-2">
                <Badge variant="outline">
                  {view.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
                  Read by {view.model}
                  {view.source === "recorded" ? " (recorded)" : ""}
                </Badge>
              </div>
              <div className="mt-3 flex max-w-sm flex-col gap-1">
                <Label htmlFor="wb-name">Sequence name (letters, digits and underscores)</Label>
                <Input
                  id="wb-name"
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value.trim() })}
                  className="font-mono"
                />
              </div>
            </div>
            <Button variant="ghost" onClick={onRestart}>
              <RotateCcw aria-hidden="true" /> Start over
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="wb-desc">Task for Muse Spark (its own words; edit to settle open questions)</Label>
            <textarea
              id="wb-desc"
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              rows={7}
              className="w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
            />
          </div>
          {draft.open_questions.length > 0 && (
            <Alert>
              <MessageCircleQuestion aria-hidden="true" />
              <AlertTitle>The document leaves {draft.open_questions.length} things open</AlertTitle>
              <AlertDescription>
                <ul className="mt-1 list-disc pl-4">
                  {draft.open_questions.map((q) => (
                    <li key={q}>{q}</li>
                  ))}
                </ul>
                <p className="mt-2">
                  The proposed scenarios avoid these where they can. Settle any that matter by editing the task above.
                </p>
              </AlertDescription>
            </Alert>
          )}
          {view.problems.length > 0 && (
            <Alert variant="destructive">
              <CircleAlert aria-hidden="true" />
              <AlertTitle>Problems the checks found</AlertTitle>
              <AlertDescription>
                <ul className="list-disc pl-4">
                  {view.problems.map((p) => (
                    <li key={p}>{p}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          )}
          <div className="grid gap-6 xl:grid-cols-2">
            <PointList title="Input" points={draft.inputs} />
            <PointList title="Output" points={draft.outputs} />
          </div>
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Parameter</TableHead>
                  <TableHead>Value</TableHead>
                  <TableHead>Meaning</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {draft.params.map((p) => (
                  <TableRow key={p.name}>
                    <TableCell className="font-mono text-xs">{p.name}</TableCell>
                    <TableCell>{typeof p.value === "number" ? formatWithUnit(p.value, p.unit) : String(p.value)}</TableCell>
                    <TableCell className="whitespace-normal">{p.meaning}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Proposed test scenarios</CardTitle>
          <CardDescription className="max-w-prose">
            Muse Spark computed these expected values from the document; they are only as right as your review. Turn off
            any you disagree with, or correct a value. Times are from the start of each scenario, checked every{" "}
            {draft.step_s} s.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {draft.scenarios.map((s) => {
            const on = kept.has(s.name);
            return (
              <section key={s.name} aria-label={s.name} className="flex flex-col gap-3 rounded-lg border p-4" data-off={!on || undefined}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="font-mono text-sm">{s.name}</p>
                    <p className="text-sm text-muted-foreground">{s.description}</p>
                    {s.rationale && <p className="mt-1 text-xs text-muted-foreground">Why: {s.rationale}</p>}
                  </div>
                  <label className="flex items-center gap-2 text-sm">
                    <Switch
                      checked={on}
                      onCheckedChange={(v) =>
                        setKept((old) => {
                          const next = new Set(old);
                          if (v) next.add(s.name);
                          else next.delete(s.name);
                          return next;
                        })
                      }
                      aria-label={`Keep ${s.name}`}
                    />
                    Keep
                  </label>
                </div>
                <div className={on ? "" : "opacity-40"}>
                  <ScenarioTimeline scenario={s} inputs={draft.inputs} outputs={draft.outputs} horizon={draft.horizon_s} />
                  <div className="overflow-x-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>Output</TableHead>
                          <TableHead>From</TableHead>
                          <TableHead>To</TableHead>
                          <TableHead>Expected</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {s.expect.map((e, i) => (
                          <TableRow key={`${e.output}-${i}`}>
                            <TableCell className="font-mono text-xs">{e.output}</TableCell>
                            <TableCell className="tabular-nums">{clock(e.from_s)}</TableCell>
                            <TableCell className="tabular-nums">{clock(e.to_s)}</TableCell>
                            <TableCell>
                              <ExpectedInput
                                value={e.equals as number | boolean}
                                unit={unitOf(e.output)}
                                label={`Expected ${e.output} in ${s.name}, check ${i + 1}`}
                                onChange={(v) => setDraft((d) => withExpected(d, s.name, i, v))}
                              />
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                </div>
              </section>
            );
          })}
          {accept.state.status === "error" && <ErrorNote error={accept.state.error} />}
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={() => accept.run({ draft: acceptedDraft(draft, kept), key: view.key })}
              disabled={kept.size === 0 || accept.state.status === "running"}
            >
              {accept.state.status === "running" && <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" />}
              Accept {kept.size} scenario{kept.size === 1 ? "" : "s"} and create the sequence
            </Button>
            <span className="text-xs text-muted-foreground">
              Then have Muse Spark draft it; only a draft that passes all accepted scenarios on the engine is verified.
            </span>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/** Where the accepted sequence stands, from the server; reloaded each time the page shows. */
function useAcceptedFacts(spec: string | null, active: boolean) {
  const drafts = useResource<DraftInfo[]>(spec ? "/api/drafts" : null);
  // A sequence with no confirmed binding answers 404: "not bound yet", not an error to show.
  const binding = useResource<BindingView>(spec ? `/api/workbench/${encodeURIComponent(spec)}/binding` : null);
  const { reload: reloadDrafts } = drafts;
  const { reload: reloadBinding } = binding;
  useEffect(() => {
    if (active && spec) {
      reloadDrafts();
      reloadBinding();
    }
  }, [active, spec, reloadDrafts, reloadBinding]);
  const verified = drafts.status === "ready" ? (drafts.data.find((d) => d.spec === spec)?.content_id ?? null) : null;
  return { verified, bound: binding.status === "ready" };
}

/** After accepting: the sequence lives on its own page; this reading cannot be accepted twice. */
function AcceptedStep({ spec, onOpen, onRestart }: { spec: string; onOpen: () => void; onRestart: () => void }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">Sequence created</CardTitle>
        <CardDescription className="max-w-prose">
          <span className="font-mono">{spec}</span> was created from this reading with the scenarios you accepted.
          Drafting, binding and runs continue on its own page.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-3">
        <Button onClick={onOpen}>Open {spec}</Button>
        <Button variant="ghost" onClick={onRestart}>
          <RotateCcw aria-hidden="true" />
          Read another document
        </Button>
      </CardContent>
    </Card>
  );
}

export function WorkbenchPage({
  onOpenSpec,
  active = true,
}: {
  onOpenSpec: (spec: string) => void;
  /** Whether this page is on screen (it stays mounted under a hidden tab). */
  active?: boolean;
}) {
  const read = useReadSequence();
  const state = read.state;
  // The sequence accepted from this reading: the page stays mounted while the spec page shows,
  // and coming back must not offer "Accept" as the next step again.
  const [accepted, setAccepted] = useState<{ key: string; spec: string } | null>(null);
  // The sequence name a sample suggested for the reading it started.
  const [suggested, setSuggested] = useState<string | null>(null);
  const readKey = state.status === "done" ? state.data.key : null;
  const spec = accepted !== null && accepted.key === readKey ? accepted.spec : null;
  const facts = useAcceptedFacts(spec, active);
  const bar = <JourneyBar read={state.status === "done"} spec={spec} verified={facts.verified} bound={facts.bound} />;
  if (state.status === "done") {
    const key = state.data.key;
    return (
      <div className="flex flex-col gap-4">
        {bar}
        {spec !== null ? (
          <AcceptedStep spec={spec} onOpen={() => onOpenSpec(spec)} onRestart={read.reset} />
        ) : (
          <ReviewStep
            key={key}
            view={state.data}
            name={suggested}
            onAccepted={(name) => {
              setAccepted({ key, spec: name });
              onOpenSpec(name);
            }}
            onRestart={read.reset}
          />
        )}
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4">
      {bar}
      <SourceStep
        onRead={(text, attachments, sequence) => {
          setSuggested(sequence);
          read.run({ text, attachments });
        }}
      />
      {state.status === "running" && (
        <p role="status" className="flex items-center gap-2 text-sm text-muted-foreground">
          <LoaderCircle aria-hidden="true" className="size-4 animate-spin motion-reduce:animate-none" />
          Muse Spark is reading the document (usually under a minute): <Elapsed since={state.since} />
        </p>
      )}
      {state.status === "error" && <ErrorNote error={state.error} />}
    </div>
  );
}
