import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { LineChart, placeEndLabels, type Series } from "./LineChart";

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

  it("marks a value that has no neighbours, and says when a chart has no data", () => {
    const lone: Series[] = [{ id: "a", label: "a", color: "red", points: [{ x: 0, y: null }, { x: 1, y: 5 }, { x: 2, y: null }] }];
    const { container } = render(<LineChart title="T" unit="u" series={lone} formatX={String} formatY={String} />);
    expect(container.querySelectorAll("circle")).toHaveLength(1);
    cleanup();
    const none: Series[] = [{ id: "a", label: "a", color: "red", points: [{ x: 0, y: null }] }];
    render(<LineChart title="T" unit="u" series={none} formatX={String} formatY={String} />);
    expect(screen.getByText("no data")).toBeTruthy();
  });

  it("writes distinct, short ticks for a nearly flat series", () => {
    const flat: Series[] = [
      { id: "a", label: "a", color: "red", points: [{ x: 0, y: 12.560006 }, { x: 1, y: 12.560001 }] },
    ];
    const fmt = (v: number) => v.toFixed(2);
    const { container } = render(<LineChart title="T" unit="°C" series={flat} formatX={String} formatY={fmt} />);
    const ticks = [...container.querySelectorAll("text[text-anchor='end'][x='42']")].map((t) => t.textContent ?? "");
    expect(ticks.length).toBeGreaterThan(1);
    expect(new Set(ticks).size).toBe(ticks.length);
    for (const t of ticks) expect(t).toMatch(/^12\.\d{1,3}$/);
  });

  it("leaves out the unit brackets when there is no unit", () => {
    render(<LineChart title="Requests" unit="" series={series} formatX={String} formatY={String} />);
    expect(screen.getByText("Requests").textContent).toBe("Requests");
  });
});

describe("placeEndLabels", () => {
  it("keeps labels apart and inside the plot, even when crowded at the bottom", () => {
    const ys = placeEndLabels([95, 99, 100, 100], 4, 100);
    expect(ys.at(-1)).toBeLessThanOrEqual(100);
    for (let i = 1; i < ys.length; i++) expect((ys[i] as number) - (ys[i - 1] as number)).toBeGreaterThanOrEqual(11);
    expect(placeEndLabels([10, 50], 4, 100)).toEqual([10, 50]);
  });
});
