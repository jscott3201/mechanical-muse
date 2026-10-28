import { useSyncExternalStore } from "react";

import { request } from "@/lib/api";
import type { Health } from "@/types/generated/Health";

import { asFailure, type Resource } from "./useResource";

/** How often the header and the run forms ask again: a restarted or reconfigured server shows. */
export const HEALTH_REFRESH_MS = 30_000;

/**
 * One `/api/health` for the whole page: the header, the run forms and the overview read the same
 * answer from one request loop, which runs while anything is mounted and stops (and forgets)
 * when nothing is.
 */
let state: Resource<Health> = { status: "loading" };
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let inFlight: AbortController | null = null;

function set(next: Resource<Health>) {
  state = next;
  for (const l of listeners) l();
}

function fetchHealth() {
  inFlight?.abort();
  const abort = new AbortController();
  inFlight = abort;
  request<Health>("GET", "/api/health", undefined, abort.signal)
    .then((data) => {
      if (!abort.signal.aborted) set({ status: "ready", data });
    })
    .catch((e: unknown) => {
      if (!abort.signal.aborted) set({ status: "error", error: asFailure(e) });
    });
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) {
    fetchHealth();
    timer = setInterval(fetchHealth, HEALTH_REFRESH_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0) return;
    if (timer !== null) clearInterval(timer);
    timer = null;
    inFlight?.abort();
    inFlight = null;
    state = { status: "loading" };
  };
}

const snapshot = () => state;

/** What the server can do right now, refreshed every `HEALTH_REFRESH_MS`. */
export function useHealth(): Resource<Health> & { reload: () => void } {
  const current = useSyncExternalStore(subscribe, snapshot, snapshot);
  return { ...current, reload: fetchHealth };
}
