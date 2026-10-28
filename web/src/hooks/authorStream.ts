/**
 * Pure state for one authoring session's event stream: the settings, one entry per attempt
 * (drafting, Spark's own tool checks if any, then checked), and the end. Nothing is trimmed; a session has at most a few
 * attempts.
 */
import type { AttemptView } from "@/types/generated/AttemptView";
import type { AuthorConfig } from "@/types/generated/AuthorConfig";
import type { AuthorEnvelope } from "@/types/generated/AuthorEnvelope";
import type { AuthorState } from "@/types/generated/AuthorState";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { TokenUsage } from "@/types/generated/TokenUsage";
import type { ToolCallView } from "@/types/generated/ToolCallView";

export interface AttemptEntry {
  index: number;
  of: number;
  /** Null while the model call is under way. */
  view: AttemptView | null;
  /** Spark's own checks while drafting, in order (tools sessions only). Not the receipt. */
  checks: ToolCallView[];
}

export interface AuthorStreamState {
  sessionId: string | null;
  config: AuthorConfig | null;
  attempts: AttemptEntry[];
  ended: {
    state: AuthorState;
    draft: DraftInfo | null;
    message: string | null;
    usage: TokenUsage;
  } | null;
  lastSeq: number | null;
  /** True once an event arrived out of order. */
  gap: boolean;
}

export const initialAuthorStream: AuthorStreamState = {
  sessionId: null,
  config: null,
  attempts: [],
  ended: null,
  lastSeq: null,
  gap: false,
};

export function reduceAuthor(state: AuthorStreamState, envs: AuthorEnvelope[]): AuthorStreamState {
  let next = state;
  for (const env of envs) {
    if (next.lastSeq !== null && env.seq <= next.lastSeq) continue;
    const gap = next.gap || (next.lastSeq === null ? env.seq !== 0 : env.seq !== next.lastSeq + 1);
    const base = { ...next, sessionId: env.session_id, lastSeq: env.seq, gap };
    switch (env.kind) {
      case "started":
        next = { ...base, config: env.config };
        break;
      case "drafting":
        next = {
          ...base,
          attempts: [
            ...base.attempts.filter((a) => a.index !== env.attempt),
            { index: env.attempt, of: env.of, view: null, checks: [] },
          ],
        };
        break;
      case "tool-call": {
        const prior = base.attempts.find((a) => a.index === env.attempt);
        const entry: AttemptEntry = prior ?? { index: env.attempt, of: base.config?.max_attempts ?? 0, view: null, checks: [] };
        next = {
          ...base,
          attempts: [
            ...base.attempts.filter((a) => a.index !== env.attempt),
            { ...entry, checks: [...entry.checks, env.call] },
          ].sort((a, b) => a.index - b.index),
        };
        break;
      }
      case "checked": {
        const prior = base.attempts.find((a) => a.index === env.attempt.index);
        const of = prior?.of ?? base.config?.max_attempts ?? 0;
        // The checked attempt carries the full list; logs recorded before tools have none.
        const recorded = (env.attempt.tool_calls as ToolCallView[] | undefined) ?? [];
        const checks = recorded.length > 0 ? recorded : (prior?.checks ?? []);
        next = {
          ...base,
          attempts: [
            ...base.attempts.filter((a) => a.index !== env.attempt.index),
            { index: env.attempt.index, of, view: env.attempt, checks },
          ].sort((a, b) => a.index - b.index),
        };
        break;
      }
      case "ended":
        next = {
          ...base,
          ended: {
            state: env.state,
            draft: env.draft ?? null,
            message: env.message ?? null,
            usage: env.usage,
          },
        };
        break;
      default: {
        const unknown: never = env;
        void unknown;
        next = base;
      }
    }
  }
  return next;
}
