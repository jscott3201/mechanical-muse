import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { ScenarioReport } from "@/types/generated/ScenarioReport";

import { Outcome } from "./Receipt";

afterEach(cleanup);

const fail: ScenarioReport = {
  name: "warm-day",
  ticks: 10,
  result: "fail",
  t: 600,
  output: "TSupSet",
  expected: 285.93,
  actual: 288.71,
  from_s: 540,
  to_s: 600,
};

describe("Outcome", () => {
  it("shows a failed temperature in kelvin with °C and °F beside it", () => {
    render(<Outcome s={fail} units={{ TSupSet: "K" }} />);
    expect(screen.getByText(/expected 285\.93 K \(12\.78 °C, 55 °F\), got 288\.71 K \(15\.56 °C, 60 °F\)/)).toBeTruthy();
  });

  it("shows the bare value when the unit is unknown", () => {
    render(<Outcome s={fail} />);
    expect(screen.getByText(/expected 285\.93, got 288\.71/)).toBeTruthy();
  });
});
