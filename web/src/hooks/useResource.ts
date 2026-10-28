import { useCallback, useEffect, useRef, useState } from "react";

import { ApiFailure, request } from "@/lib/api";

export type Resource<T> =
  | { status: "loading" }
  | { status: "error"; error: ApiFailure }
  | { status: "ready"; data: T };

export function asFailure(e: unknown): ApiFailure {
  if (e instanceof ApiFailure) return e;
  return new ApiFailure(0, "network", "cannot reach mm-server");
}

/**
 * GETs `path` (or nothing while it is null); aborts on change or unmount. A reload of the same
 * path keeps showing the data it has until the new data arrives, so views built on it do not
 * unmount and remount (a live stream would be dropped).
 */
export function useResource<T>(path: string | null): Resource<T> & { reload: () => void } {
  const [state, setState] = useState<Resource<T>>({ status: "loading" });
  const [version, setVersion] = useState(0);
  const loadedPath = useRef<string | null>(null);
  useEffect(() => {
    if (path === null) return;
    const abort = new AbortController();
    if (loadedPath.current !== path) {
      loadedPath.current = path;
      setState({ status: "loading" });
    }
    request<T>("GET", path, undefined, abort.signal)
      .then((data) => setState({ status: "ready", data }))
      .catch((e: unknown) => {
        if (abort.signal.aborted) return;
        setState({ status: "error", error: asFailure(e) });
      });
    return () => abort.abort();
  }, [path, version]);
  const reload = useCallback(() => setVersion((v) => v + 1), []);
  return { ...state, reload };
}

export type Action<T> =
  | { status: "idle" }
  | { status: "running" }
  | { status: "error"; error: ApiFailure }
  | { status: "done"; data: T };

/** A POST the user triggers; one at a time. */
export function useAction<T>(path: string | null, body?: unknown) {
  const [state, setState] = useState<Action<T>>({ status: "idle" });
  // A reply only lands if it answers the latest request for the current path; a slow reply
  // for an earlier path must never show under a later one.
  const latest = useRef(0);
  useEffect(() => {
    latest.current += 1;
    setState({ status: "idle" });
  }, [path]);
  const run = useCallback(() => {
    if (path === null) return;
    const ticket = ++latest.current;
    setState({ status: "running" });
    request<T>("POST", path, body)
      .then((data) => {
        if (latest.current === ticket) setState({ status: "done", data });
      })
      .catch((e: unknown) => {
        if (latest.current === ticket) setState({ status: "error", error: asFailure(e) });
      });
  }, [path, body]);
  return { state, run };
}
