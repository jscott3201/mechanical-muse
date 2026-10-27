/**
 * Pure state for one authoring session's event stream: the settings, one entry per attempt
 * (drafting, then checked), and the end. Nothing is trimmed; a session has at most a few
 * attempts.
 */
import type { AttemptView } from "@/types/generated/AttemptView";
import type { AuthorConfig } from "@/types/generated/AuthorConfig";
import type { AuthorEnvelope } from "@/types/generated/AuthorEnvelope";
import type { AuthorState } from "@/types/generated/AuthorState";
import type { DraftInfo } from "@/types/generated/DraftInfo";
import type { TokenUsage } from "@/types/generated/TokenUsage";

export interface AttemptEntry {
  index: number;
  of: number;
  /** Null while the model call is under way. */
  view: AttemptView | null;
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
            { index: env.attempt, of: env.of, view: null },
          ],
        };
        break;
      case "checked": {
        const of = base.attempts.find((a) => a.index === env.attempt.index)?.of ?? base.config?.max_attempts ?? 0;
        next = {
          ...base,
          attempts: [
            ...base.attempts.filter((a) => a.index !== env.attempt.index),
            { index: env.attempt.index, of, view: env.attempt },
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
