/** Where the app is, as kept in the URL hash so links, reloads and Back work without a router. */

export const SECTIONS = ["interface", "graph", "spark", "building", "scenarios", "task"] as const;
export type Section = (typeof SECTIONS)[number];

export type Page = "overview" | "sequences" | "workbench" | "runs";

export type Route =
  | { page: "overview" }
  | { page: "sequences"; spec?: string; section?: Section }
  | { page: "workbench" }
  /** `draft` presets the start form; `replay` points at a recorded run (never starts it). */
  | { page: "runs"; run?: string; draft?: { spec: string; contentId: string }; replay?: string };

const OVERVIEW: Route = { page: "overview" };

function decode(segment: string): string | null {
  try {
    const s = decodeURIComponent(segment);
    return s.trim() === "" ? null : s;
  } catch {
    return null;
  }
}

function isSection(s: string): s is Section {
  return (SECTIONS as readonly string[]).includes(s);
}

/** Reads a hash such as `#/sequences/chw_plant_reset/graph`. Anything unknown is the overview. */
export function parse(hash: string): Route {
  const body = hash.replace(/^#/, "").replace(/^\//, "");
  const q = body.indexOf("?");
  const path = q === -1 ? body : body.slice(0, q);
  const query = new URLSearchParams(q === -1 ? "" : body.slice(q + 1));
  const parts = path.split("/").filter((p) => p !== "");
  const [head, a, b] = parts;
  switch (head) {
    case undefined:
      return OVERVIEW;
    case "workbench":
      return parts.length === 1 ? { page: "workbench" } : OVERVIEW;
    case "sequences": {
      if (parts.length > 3) return OVERVIEW;
      if (a === undefined) return { page: "sequences" };
      const spec = decode(a);
      if (spec === null) return OVERVIEW;
      if (b === undefined) return { page: "sequences", spec };
      return isSection(b) ? { page: "sequences", spec, section: b } : { page: "sequences", spec };
    }
    case "runs": {
      if (parts.length > 2) return OVERVIEW;
      if (a !== undefined) {
        const run = decode(a);
        return run === null ? OVERVIEW : { page: "runs", run };
      }
      const spec = query.get("spec");
      const contentId = query.get("draft");
      const replay = query.get("replay");
      return {
        page: "runs",
        ...(spec && contentId ? { draft: { spec, contentId } } : {}),
        ...(replay ? { replay } : {}),
      };
    }
    default:
      return OVERVIEW;
  }
}

/** The hash for `route`, the inverse of `parse`. */
export function format(route: Route): string {
  const enc = encodeURIComponent;
  switch (route.page) {
    case "overview":
      return "#/";
    case "workbench":
      return "#/workbench";
    case "sequences":
      if (!route.spec) return "#/sequences";
      return `#/sequences/${enc(route.spec)}${route.section ? `/${route.section}` : ""}`;
    case "runs": {
      if (route.run) return `#/runs/${enc(route.run)}`;
      const query = new URLSearchParams();
      if (route.draft) {
        query.set("spec", route.draft.spec);
        query.set("draft", route.draft.contentId);
      }
      if (route.replay) query.set("replay", route.replay);
      const qs = query.toString();
      return qs ? `#/runs?${qs}` : "#/runs";
    }
  }
}
