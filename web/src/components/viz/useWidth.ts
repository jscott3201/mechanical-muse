import { useCallback, useEffect, useState } from "react";

/**
 * The width of whichever element the returned ref is attached to, following resizes. A callback
 * ref, so a remounted element (a chart shown again after its table view) is measured afresh.
 */
export function useWidth(fallback = 480): [(el: HTMLElement | null) => void, number] {
  const [el, setEl] = useState<HTMLElement | null>(null);
  const [width, setWidth] = useState(fallback);
  const ref = useCallback((node: HTMLElement | null) => setEl(node), []);
  useEffect(() => {
    if (!el) return;
    setWidth(el.clientWidth || fallback);
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0) setWidth(entry.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [el, fallback]);
  return [ref, width];
}
