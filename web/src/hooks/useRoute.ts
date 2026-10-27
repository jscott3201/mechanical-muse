import { useCallback, useMemo, useSyncExternalStore } from "react";

import { format, parse, type Route } from "@/lib/route";

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const snapshot = () => window.location.hash;

/** Sets the hash, so the browser keeps history and Back returns to the previous view. */
export function navigate(route: Route): void {
  const hash = format(route);
  if (window.location.hash !== hash) window.location.hash = hash;
}

/** The current route, following the URL hash. */
export function useRoute(): [Route, (route: Route) => void] {
  const hash = useSyncExternalStore(subscribe, snapshot, () => "");
  const route = useMemo(() => parse(hash), [hash]);
  const go = useCallback((r: Route) => navigate(r), []);
  return [route, go];
}
