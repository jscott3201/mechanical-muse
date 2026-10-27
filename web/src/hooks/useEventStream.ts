import { useEffect, useReducer, useState } from "react";

export type Connection = "idle" | "connecting" | "open" | "reconnecting" | "closed";

type Msg<E> = { type: "reset" } | { type: "batch"; events: E[] };

const nextFrame: (cb: () => void) => void =
  typeof requestAnimationFrame === "function"
    ? (cb) => requestAnimationFrame(cb)
    : (cb) => setTimeout(cb, 16);

/**
 * Follows one server-sent event stream whose events are JSON with a `kind`. The browser's
 * EventSource reconnects by itself and sends `Last-Event-ID`; the server answers 204 once the
 * stream has ended, which closes it. Events are applied in batches, once per animation frame,
 * by a pure `reduce`. The source is closed on unmount and after the `ended` event.
 */
export function useEventStream<E extends { kind: string }, S>(
  url: string | null,
  kinds: readonly string[],
  reduce: (state: S, events: E[]) => S,
  initial: S,
): S & { connection: Connection } {
  const [state, dispatch] = useReducer(
    (s: S, msg: Msg<E>) => (msg.type === "reset" ? initial : reduce(s, msg.events)),
    initial,
  );
  const [connection, setConnection] = useState<Connection>("idle");
  // `kinds` is a constant list per caller; join it so a new array each render is no change.
  const kindKey = kinds.join(",");
  useEffect(() => {
    dispatch({ type: "reset" });
    if (url === null) {
      setConnection("idle");
      return;
    }
    setConnection("connecting");
    const source = new EventSource(url);
    let pending: E[] = [];
    let scheduled = false;
    let closed = false;
    const flush = () => {
      scheduled = false;
      if (closed || pending.length === 0) return;
      const events = pending;
      pending = [];
      dispatch({ type: "batch", events });
    };
    const onEvent = (ev: MessageEvent<string>) => {
      let parsed: E;
      try {
        parsed = JSON.parse(ev.data) as E;
      } catch {
        return;
      }
      pending.push(parsed);
      if (parsed.kind === "ended") {
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
    for (const kind of kindKey.split(",")) source.addEventListener(kind, onEvent as EventListener);
    source.onopen = () => setConnection("open");
    source.onerror = () =>
      setConnection(source.readyState === EventSource.CLOSED ? "closed" : "reconnecting");
    return () => {
      closed = true;
      source.close();
    };
  }, [url, kindKey]);
  return { ...state, connection };
}
