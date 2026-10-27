import { type RefObject, useEffect, useState } from "react";

/** The element's width, following resizes (charts redraw to fit). */
export function useWidth(ref: RefObject<HTMLElement | null>, fallback = 480): number {
  const [width, setWidth] = useState(fallback);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth || fallback);
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) setWidth(entry.contentRect.width || fallback);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, fallback]);
  return width;
}
