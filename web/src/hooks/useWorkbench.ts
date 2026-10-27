import { useCallback, useRef, useState } from "react";

import { ApiFailure, request } from "@/lib/api";
import type { AcceptView } from "@/types/generated/AcceptView";
import type { ReadRequest } from "@/types/generated/ReadRequest";
import type { ReadView } from "@/types/generated/ReadView";
import type { SequenceDraft } from "@/types/generated/SequenceDraft";

type Call<T> =
  | { status: "idle" }
  | { status: "running"; since: number }
  | { status: "error"; error: ApiFailure }
  | { status: "done"; data: T };

function asFailure(e: unknown): ApiFailure {
  return e instanceof ApiFailure ? e : new ApiFailure(0, "network", "cannot reach mm-server");
}

/** A POST whose body is given when it runs; only the latest call's reply lands. */
function useCall<B, T>(path: string) {
  const [state, setState] = useState<Call<T>>({ status: "idle" });
  const latest = useRef(0);
  const run = useCallback(
    (body: B) => {
      const ticket = ++latest.current;
      setState({ status: "running", since: Date.now() });
      request<T>("POST", path, body)
        .then((data) => latest.current === ticket && setState({ status: "done", data }))
        .catch((e: unknown) => latest.current === ticket && setState({ status: "error", error: asFailure(e) }));
    },
    [path],
  );
  const reset = useCallback(() => {
    latest.current += 1;
    setState({ status: "idle" });
  }, []);
  return { state, run, reset };
}

/** Muse Spark reads a sequence document (`/api/workbench/read`). */
export function useReadSequence() {
  return useCall<ReadRequest, ReadView>("/api/workbench/read");
}

/** The engineer accepts the reviewed draft as a sequence (`/api/workbench/accept`). */
export function useAcceptSequence() {
  return useCall<{ draft: SequenceDraft; key: string | null }, AcceptView>("/api/workbench/accept");
}
