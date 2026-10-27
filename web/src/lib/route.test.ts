import { describe, expect, it } from "vitest";

import { format, parse, type Route } from "./route";

describe("route", () => {
  const cases: [string, Route][] = [
    ["#/", { page: "overview" }],
    ["#/sequences", { page: "sequences" }],
    ["#/sequences/chw_plant_reset", { page: "sequences", spec: "chw_plant_reset" }],
    ["#/sequences/chw_plant_reset/graph", { page: "sequences", spec: "chw_plant_reset", section: "graph" }],
    ["#/workbench", { page: "workbench" }],
    ["#/runs", { page: "runs" }],
    ["#/runs/run-7", { page: "runs", run: "run-7" }],
    ["#/runs?spec=ahu1_sat_stpt&draft=cxf%3Afnv1a128%3Aabc", { page: "runs", draft: { spec: "ahu1_sat_stpt", contentId: "cxf:fnv1a128:abc" } }],
    ["#/runs?replay=day185-noon-your-sequence", { page: "runs", replay: "day185-noon-your-sequence" }],
  ];

  it.each(cases)("parses and formats %s", (hash, route) => {
    expect(parse(hash)).toEqual(route);
    expect(format(route)).toBe(hash);
  });

  it("treats an empty hash as the overview", () => {
    expect(parse("")).toEqual({ page: "overview" });
    expect(parse("#")).toEqual({ page: "overview" });
  });

  it("sends anything it does not know to the overview", () => {
    for (const bad of ["#/nowhere", "#/workbench/x", "#/sequences/a/b/c", "#/runs/a/b", "#/sequences/%E0%A4%A", "#/sequences/%20"]) {
      expect(parse(bad)).toEqual({ page: "overview" });
    }
  });

  it("drops an unknown section and a draft without its spec", () => {
    expect(parse("#/sequences/x/nope")).toEqual({ page: "sequences", spec: "x" });
    expect(parse("#/runs?draft=cxf:1")).toEqual({ page: "runs" });
  });

  it("round-trips spec names that need escaping", () => {
    const route: Route = { page: "sequences", spec: "a b/c", section: "spark" };
    expect(parse(format(route))).toEqual(route);
  });
});
