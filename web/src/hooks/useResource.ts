import { useCallback, useEffect, useState } from "react";

import { ApiFailure, request } from "@/lib/api";

export type Resource<T> =
  | { status: "loading" }
  | { status: "error"; error: ApiFailure }
  | { status: "ready"; data: T };

function asFailure(e: unknown): ApiFailure {
  if (e instanceof ApiFailure) return e;
  return new ApiFailure(0, "network", "cannot reach mm-server");
}

/** GETs `path` (or nothing while it is null); aborts on change or unmount. */
export function useResource<T>(path: string | null): Resource<T> & { reload: () => void } {
  const [state, setState] = useState<Resource<T>>({ status: "loading" });
  const [version, setVersion] = useState(0);
  useEffect(() => {
    if (path === null) return;
    const abort = new AbortController();
    setState({ status: "loading" });
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
  useEffect(() => setState({ status: "idle" }), [path]);
  const run = useCallback(() => {
    if (path === null) return;
    setState({ status: "running" });
    request<T>("POST", path, body)
      .then((data) => setState({ status: "done", data }))
      .catch((e: unknown) => setState({ status: "error", error: asFailure(e) }));
  }, [path, body]);
  return { state, run };
}
