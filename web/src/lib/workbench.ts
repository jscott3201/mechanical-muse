/** Pure helpers for the bring-your-own-sequence workbench. */

import type { ScenarioDraft } from "@/types/generated/ScenarioDraft";
import type { SequenceDraft } from "@/types/generated/SequenceDraft";
import type { WorkbenchAttachment } from "@/types/generated/WorkbenchAttachment";

export const ACCEPTED_TYPES = ["application/pdf", "image/png", "image/jpeg", "image/gif", "image/webp"];
/** The server takes 20 MB per request; base64 adds a third, so about 14 MB of files in all. */
export const MAX_TOTAL_BYTES = 14 * 1024 * 1024;

/** Base64 of bytes, without a data-URL prefix. Chunked so large files do not overflow the stack. */
export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export async function fileToAttachment(file: Blob, name: string): Promise<WorkbenchAttachment> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  return { name, mime: file.type, data_base64: toBase64(bytes) };
}

/** Why a file cannot be attached, or null when it can. */
export function fileProblem(file: { type: string; size: number; name: string }): string | null {
  if (!ACCEPTED_TYPES.includes(file.type)) return `${file.name}: attach a PDF or an image (PNG, JPEG, GIF, WebP)`;
  if (file.size > MAX_TOTAL_BYTES) return `${file.name}: larger than 14 MB`;
  return null;
}

/** Why adding a file of `size` bytes to `attached` would make the request too large. */
export function totalProblem(attached: WorkbenchAttachment[], size: number): string | null {
  const bytes = attached.reduce((n, a) => n + Math.floor((a.data_base64.length * 3) / 4), 0) + size;
  return bytes > MAX_TOTAL_BYTES ? "Attachments are limited to 14 MB in all" : null;
}

/** The draft with only the kept scenarios, ready to accept. */
export function acceptedDraft(draft: SequenceDraft, kept: ReadonlySet<string>): SequenceDraft {
  return { ...draft, scenarios: draft.scenarios.filter((s) => kept.has(s.name)) };
}

/** Replaces one expectation's expected value in one scenario. */
export function withExpected(
  draft: SequenceDraft,
  scenario: string,
  index: number,
  equals: number | boolean,
): SequenceDraft {
  return {
    ...draft,
    scenarios: draft.scenarios.map((s) =>
      s.name !== scenario
        ? s
        : { ...s, expect: s.expect.map((e, i) => (i === index ? { ...e, equals } : e)) },
    ),
  };
}

/** A time in seconds as "m:ss" or "h:mm:ss" from the start of a scenario. */
export function clock(seconds: number): string {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** The last time any input changes or any check ends, for scaling a scenario's timeline. */
export function scenarioEnd(s: ScenarioDraft, horizon: number): number {
  const times = [
    ...s.inputs.flatMap((i) => i.steps.map((st) => st.t)),
    ...s.expect.map((e) => e.to_s),
  ];
  const end = Math.max(0, ...times);
  return Math.min(horizon, Math.max(end * 1.05, 60));
}
