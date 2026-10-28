import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { KpiTiles } from "./RunSummary";

afterEach(cleanup);

describe("KpiTiles", () => {
  it("names the run it compares with, so a faulted run never reads as a saving", () => {
    render(<KpiTiles kpi={{ ener_tot: 0.101 }} base={{ ener_tot: 0.1 }} against="run without the fault" />);
    expect(screen.getByLabelText("Change against the run without the fault")).toBeTruthy();
    expect(screen.getByText("Energy vs run without the fault")).toBeTruthy();
    expect(screen.getByText("+1 %")).toBeTruthy();
    expect(screen.getByText("Higher; lower is better")).toBeTruthy();
  });

  it("compares with the baseline run by default", () => {
    render(<KpiTiles kpi={{ ener_tot: 0.09 }} base={{ ener_tot: 0.1 }} />);
    expect(screen.getByLabelText("Change against the baseline run")).toBeTruthy();
    expect(screen.getByText("Energy vs baseline")).toBeTruthy();
  });
});
