import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { LineChart, type Series } from "./LineChart";

afterEach(cleanup);

const series: Series[] = [
  { id: "a", label: "setpoint sent", color: "var(--viz-1)", points: [{ x: 0, y: 1 }, { x: 60, y: null }, { x: 120, y: 3 }] },
  { id: "b", label: "measured", color: "var(--viz-2)", points: [{ x: 0, y: 2 }, { x: 60, y: 2 }, { x: 120, y: 2 }] },
];

describe("LineChart", () => {
  it("breaks the line where data is missing and labels every series", () => {
    const { container } = render(
      <LineChart title="Temp" unit="°C" series={series} formatX={String} formatY={String} />,
    );
    const paths = [...container.querySelectorAll("path")].map((p) => p.getAttribute("d") ?? "");
    // The first series has two separate pieces (two moves), the second one continuous piece.
    expect(paths[0]?.match(/M/g)).toHaveLength(2);
    expect(paths[1]?.match(/M/g)).toHaveLength(1);
    expect(screen.getAllByText("setpoint sent").length).toBeGreaterThanOrEqual(2); // legend + end label
  });

  it("has a table view that says no data", () => {
    render(<LineChart title="Temp" unit="°C" series={series} formatX={String} formatY={String} />);
    fireEvent.click(screen.getByRole("button", { name: "Show table" }));
    expect(screen.getByText("no data")).toBeTruthy();
    expect(screen.getAllByRole("row")).toHaveLength(4);
  });
});
