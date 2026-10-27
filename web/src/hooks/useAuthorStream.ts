import type { AuthorEnvelope } from "@/types/generated/AuthorEnvelope";

import { type AuthorStreamState, initialAuthorStream, reduceAuthor } from "./authorStream";
import { type Connection, useEventStream } from "./useEventStream";

const KINDS = ["started", "drafting", "checked", "ended"] as const;

/** Follows one authoring session's event stream (see `useEventStream`). */
export function useAuthorStream(
  sessionId: string | null,
): AuthorStreamState & { connection: Connection } {
  return useEventStream<AuthorEnvelope, AuthorStreamState>(
    sessionId === null ? null : `/api/authoring/${encodeURIComponent(sessionId)}/events`,
    KINDS,
    reduceAuthor,
    initialAuthorStream,
  );
}
