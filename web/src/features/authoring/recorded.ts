import type { ModelSource } from "@/types/generated/ModelSource";
import type { RecordedSessions } from "@/types/generated/RecordedSessions";

/** Why the Draft button or the tools switch is off for a recorded session; `null` when it is on. */
export type RecordedBlocks = { draft: string | null; tools: string | null };

export const NO_TOOLS_RECORDING = "No recorded session with Spark's own checks for this sequence.";
export const NO_RECORDING =
  "No recorded session for this sequence: drafting it needs a model key on the server. The chilled-water plant reset has one to play back.";
export const ONLY_TOOLS_RECORDING =
  "This sequence's recorded session is the one where Spark checks its own work: turn that on to play it back.";

/**
 * What a recorded Draft can do for this spec. Only an explicit `false` from the server blocks:
 * while the answer is loading or unavailable the server still refuses with its own reason.
 */
export function recordedBlocks(available: RecordedSessions | null, source: ModelSource, tools: boolean): RecordedBlocks {
  if (source !== "recorded" || available === null) return { draft: null, tools: null };
  const toolsBlock = available.tools === false ? NO_TOOLS_RECORDING : null;
  const wantsTools = tools && toolsBlock === null;
  let draft: string | null = null;
  if (wantsTools ? available.tools === false : available.plain === false) {
    draft = available.tools === false ? NO_RECORDING : ONLY_TOOLS_RECORDING;
  }
  return { draft, tools: toolsBlock };
}
