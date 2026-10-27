import { useEffect, useReducer, useState } from "react";

import type { Envelope } from "@/types/generated/Envelope";

import { initialRunStream, reduceRun, type RunStreamState } from "./runStream";

const KINDS = ["phase", "started", "tick", "kpi", "ended"] as const;

type Connection = "connecting" | "open" | "reconnecting" | "closed";

type Msg = { type: "reset" } | { type: "batch"; envs: Envelope[] };

function reducer(state: RunStreamState, msg: Msg): RunStreamState {
  return msg.type === "reset" ? initialRunStream : reduceRun(state, msg.envs);
}

const nextFrame: (cb: () => void) => void =
  typeof requestAnimationFrame === "function"
    ? (cb) => requestAnimationFrame(cb)
    : (cb) => setTimeout(cb, 16);

/**
 * Follows one run's event stream. The browser's EventSource reconnects by itself and sends
 * `Last-Event-ID`; the server answers 204 once the run has ended, which closes it. Events are
 * applied once per animation frame. The source is closed on unmount and after `ended`.
 */
export function useRunStream(runId: string | null): RunStreamState & { connection: Connection } {
  const [state, dispatch] = useReducer(reducer, initialRunStream);
  const [connection, setConnection] = useState<Connection>("connecting");
  useEffect(() => {
    dispatch({ type: "reset" });
    if (runId === null) return;
    setConnection("connecting");
    const source = new EventSource(`/api/runs/${encodeURIComponent(runId)}/events`);
    let pending: Envelope[] = [];
    let scheduled = false;
    let closed = false;
    const flush = () => {
      scheduled = false;
      if (closed || pending.length === 0) return;
      const envs = pending;
      pending = [];
      dispatch({ type: "batch", envs });
    };
    const onEvent = (ev: MessageEvent<string>) => {
      let env: Envelope;
      try {
        env = JSON.parse(ev.data) as Envelope;
      } catch {
        return;
      }
      pending.push(env);
      if (env.kind === "ended") {
        flush();
        source.close();
        setConnection("closed");
        return;
      }
      if (!scheduled) {
        scheduled = true;
        nextFrame(flush);
      }
    };
    for (const kind of KINDS) source.addEventListener(kind, onEvent as EventListener);
    source.onopen = () => setConnection("open");
    source.onerror = () =>
      setConnection(source.readyState === EventSource.CLOSED ? "closed" : "reconnecting");
    return () => {
      closed = true;
      source.close();
    };
  }, [runId]);
  return { ...state, connection };
}
