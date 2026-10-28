import type { DraftInfo } from "@/types/generated/DraftInfo";

/**
 * A spec's stored drafts, newest first by store order (a content id is a hash and says nothing
 * about age). The server already lists them so; sorting here keeps every view right whatever
 * order it is handed. Drafts stored before store order existed (0) come last, by content id.
 */
export function draftsOf(drafts: readonly DraftInfo[], spec: string): DraftInfo[] {
  return drafts
    .filter((d) => d.spec === spec)
    .sort((a, b) => b.stored_seq - a.stored_seq || (a.content_id < b.content_id ? -1 : a.content_id > b.content_id ? 1 : 0));
}

/** The newest stored draft of `spec`, if any. */
export function newestDraft(drafts: readonly DraftInfo[], spec: string): DraftInfo | null {
  return draftsOf(drafts, spec)[0] ?? null;
}
