import type { Route } from "./route";

/** Where an engineer's own sequence is on the way from a document to a BOPTEST run. */
export type JourneyFacts = {
  /** Muse Spark has read the document (the review step is open). */
  read: boolean;
  /** The accepted sequence's name, once it exists. */
  spec: string | null;
  /** Content id of the newest verified draft, if any. */
  verified: string | null;
  bound: boolean;
};

export type StepState = "done" | "next" | "todo";

export type JourneyStep = {
  id: "read" | "accept" | "draft" | "bind" | "run";
  label: string;
  state: StepState;
  /** Where the step is done; absent while it cannot be reached yet. */
  to?: Route;
};

/** The five steps, each marked from data the server holds; the first open one is "next". */
export function journey(f: JourneyFacts): JourneyStep[] {
  const spec = f.spec;
  const done = {
    read: f.read || spec !== null,
    accept: spec !== null,
    draft: f.verified !== null,
    bind: f.bound,
    // A run is never marked done here: the app does not track which runs used which draft.
    run: false,
  };
  const steps: Omit<JourneyStep, "state">[] = [
    { id: "read", label: "Read", to: { page: "workbench" } },
    { id: "accept", label: "Accept", to: { page: "workbench" } },
    { id: "draft", label: "Draft & verify", ...(spec ? { to: { page: "sequences", spec, section: "spark" } } : {}) },
    { id: "bind", label: "Bind", ...(spec ? { to: { page: "sequences", spec, section: "building" } } : {}) },
    {
      id: "run",
      label: "Run",
      ...(spec && f.verified && f.bound ? { to: { page: "runs", draft: { spec, contentId: f.verified } } } : {}),
    },
  ];
  let seenOpen = false;
  return steps.map((s) => {
    if (done[s.id]) return { ...s, state: "done" };
    const state: StepState = seenOpen ? "todo" : "next";
    seenOpen = true;
    return { ...s, state };
  });
}
