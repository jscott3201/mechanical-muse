import { CircleCheck, CircleX, GitBranchPlus, History, LoaderCircle, Sparkles, Swords } from "lucide-react";
import { useState } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useResource } from "@/hooks/useResource";
import { navigate } from "@/hooks/useRoute";
import { useAcceptSequence, useAttack } from "@/hooks/useWorkbench";
import { MAX_SCENARIOS, revisionDraft, revisionName } from "@/lib/workbench";
import type { AttackProposal } from "@/types/generated/AttackProposal";
import type { AttackView } from "@/types/generated/AttackView";
import type { SpecSummary } from "@/types/generated/SpecSummary";

import { ErrorNote } from "../ErrorNote";
import { Outcome, type OutputUnits } from "../Receipt";
import { ScenarioTimeline } from "./ScenarioTimeline";

const NO_MODEL_HINT =
  "This server has no model key and no recorded proposals for this sequence as it stands. Proposals need a model key on the server.";

function ResultBadge({ p }: { p: AttackProposal }) {
  return p.result === "passed" ? (
    <Badge variant="outline">
      <CircleCheck aria-hidden="true" /> Passes this draft
    </Badge>
  ) : (
    <Badge variant="destructive">
      <CircleX aria-hidden="true" /> Fails this draft
    </Badge>
  );
}

/** One proposal: why Spark proposed it, its timeline, and what the replay found. */
function ProposalCard({
  p,
  view,
  units,
  chosen,
  onChoose,
}: {
  p: AttackProposal;
  view: AttackView;
  units: OutputUnits;
  chosen: boolean;
  onChoose: (on: boolean) => void;
}) {
  const s = p.scenario;
  return (
    <li className="flex flex-col gap-2 rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <label className="flex items-center gap-2 text-sm font-medium">
          <input
            type="checkbox"
            checked={chosen}
            onChange={(e) => onChoose(e.target.checked)}
            aria-label={`Choose ${s.name}`}
          />
          <span className="font-mono">{s.name}</span>
        </label>
        <ResultBadge p={p} />
      </div>
      <p className="text-sm">{s.rationale}</p>
      {s.description && <p className="text-sm text-muted-foreground">{s.description}</p>}
      {p.result === "failed" &&
        p.failures.map((f) => (
          <p key={f.name} className="text-sm">
            <Outcome s={f} units={units} />
          </p>
        ))}
      <ScenarioTimeline
        scenario={s}
        inputs={view.sequence.inputs}
        outputs={view.sequence.outputs}
        horizon={view.sequence.horizon_s}
      />
    </li>
  );
}

/** Spark's proposals for one attack, and accepting the chosen ones as a new revision. */
function Proposals({ view }: { view: AttackView }) {
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const [name, setName] = useState<string | null>(null);
  const accept = useAcceptSequence();
  const list = useResource<SpecSummary[]>("/api/workbench");
  const taken = list.status === "ready" ? list.data.map((s) => s.name) : [];
  const revision = name ?? revisionName(view.sequence.name, taken);
  const units: OutputUnits = Object.fromEntries(view.sequence.outputs.map((o) => [o.name, o.unit]));
  const passed = view.proposals.filter((p) => p.result === "passed").length;
  const total = view.sequence.scenarios.length + chosen.size;
  const picked = view.proposals.filter((p) => chosen.has(p.scenario.name)).map((p) => p.scenario);
  const accepted = accept.state.status === "done" ? accept.state.data.spec.summary : null;
  return (
    <div className="flex flex-col gap-3">
      <Alert>
        {view.source === "recorded" ? <History aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
        <AlertTitle>
          Proposed by Muse Spark ({view.source}) — tests, not verdicts
        </AlertTitle>
        <AlertDescription>
          <p>
            Spark wrote these from your sequence, its accepted scenarios and its open questions, without seeing this
            draft. Each was replayed against draft <code className="break-all">{view.draft}</code> on the engine;
            nothing ran on the building. A failing proposal means the draft or the proposal is wrong: check it against
            your sequence before you decide. They are model-written until you accept them, and your sequence's own
            scenarios are unchanged.
          </p>
        </AlertDescription>
      </Alert>
      <p className="text-sm">
        {passed} of {view.proposals.length} proposal{view.proposals.length === 1 ? "" : "s"} pass this draft.
      </p>
      <ul className="flex flex-col gap-3" aria-label="Proposed scenarios">
        {view.proposals.map((p) => (
          <ProposalCard
            key={p.scenario.name}
            p={p}
            view={view}
            units={units}
            chosen={chosen.has(p.scenario.name)}
            onChoose={(on) =>
              setChosen((old) => {
                const next = new Set(old);
                if (on) next.add(p.scenario.name);
                else next.delete(p.scenario.name);
                return next;
              })
            }
          />
        ))}
      </ul>
      {view.problems.length > 0 && (
        <div className="text-sm text-muted-foreground">
          <p>Dropped by the checks:</p>
          <ul className="list-disc pl-5">
            {view.problems.map((m) => (
              <li key={m}>{m}</li>
            ))}
          </ul>
        </div>
      )}
      {accepted ? (
        <Alert>
          <GitBranchPlus aria-hidden="true" />
          <AlertTitle>Accepted as {accepted.name}</AlertTitle>
          <AlertDescription>
            <p>
              A new sequence with {accepted.scenarios} scenarios. Have Muse Spark draft it: the new scenarios gate its
              drafts from now on.
            </p>
            <Button size="sm" onClick={() => navigate({ page: "sequences", spec: accepted.name, section: "spark" })}>
              <Sparkles aria-hidden="true" /> Draft {accepted.name} with Muse Spark
            </Button>
          </AlertDescription>
        </Alert>
      ) : (
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="revision-name">New revision</Label>
            <Input
              id="revision-name"
              className="w-64 font-mono"
              value={revision}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <Button
            onClick={() => accept.run({ draft: revisionDraft(view.sequence, picked, revision), key: view.key })}
            disabled={chosen.size === 0 || total > MAX_SCENARIOS || accept.state.status === "running"}
          >
            <GitBranchPlus aria-hidden="true" /> Accept selected as revision {revision}
          </Button>
          {total > MAX_SCENARIOS && (
            <p className="text-sm text-muted-foreground">
              A sequence has at most {MAX_SCENARIOS} scenarios; choose fewer.
            </p>
          )}
        </div>
      )}
      {accept.state.status === "error" && <ErrorNote error={accept.state.error} />}
    </div>
  );
}

/**
 * "Try to break it": Muse Spark proposes scenarios aimed at edges the accepted ones miss, and
 * each is replayed against this verified draft. The engineer decides what to keep.
 */
export function AttackPanel({ spec, draft }: { spec: string; draft: string }) {
  const attack = useAttack(spec);
  const running = attack.state.status === "running";
  return (
    <section aria-label="Try to break it" className="flex flex-col gap-3 rounded-md border p-4">
      <div className="flex flex-col gap-1">
        <h3 className="font-medium">Try to break it</h3>
        <p className="max-w-prose text-sm text-muted-foreground">
          Muse Spark reads your written sequence, its accepted scenarios and open questions (not this draft) and
          proposes three to five new scenarios aimed at the edges they miss: exact thresholds, the ends of holds and
          delays, simultaneous changes. Each is replayed against this draft.
        </p>
      </div>
      <div>
        <Button variant="outline" onClick={() => attack.run({ draft })} disabled={running}>
          {running ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Swords aria-hidden="true" />}
          {running ? "Muse Spark is looking for edges…" : "Try to break it"}
        </Button>
      </div>
      {attack.state.status === "error" && (
        <ErrorNote
          error={attack.state.error}
          onRetry={attack.state.error.code === "no-model" ? undefined : () => attack.run({ draft })}
          hint={attack.state.error.code === "no-model" ? NO_MODEL_HINT : undefined}
        />
      )}
      {attack.state.status === "done" && <Proposals key={attack.state.data.key} view={attack.state.data} />}
    </section>
  );
}
