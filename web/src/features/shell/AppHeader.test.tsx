import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import type { Health } from "@/types/generated/Health";

import { AppHeader } from "./AppHeader";

beforeEach(() => {
  // jsdom has no layout engine; the tooltip's positioning only needs this to exist.
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const health: Health = { status: "ok", engine_rev: "fa5cb19abc", specs: 4, recorded_model: true, boptest: true };

function renderHeader() {
  render(
    <TooltipProvider>
      <AppHeader health={{ status: "ready", data: health }} theme="light" onToggleTheme={() => {}} />
    </TooltipProvider>,
  );
}

describe("AppHeader", () => {
  it("groups the status chips and opens a chip's detail from the keyboard", () => {
    renderHeader();
    expect(screen.getByRole("group", { name: "Server status" })).toBeTruthy();
    const chip = screen.getByRole("button", { name: "BOPTEST" });
    act(() => chip.focus());
    expect(screen.getAllByText("Live runs drive the BOPTEST office building").length).toBeGreaterThan(0);
  });

  it("toggles a chip's detail with a tap", () => {
    renderHeader();
    const chip = screen.getByRole("button", { name: /^engine/ });
    fireEvent.pointerDown(chip);
    fireEvent.click(chip);
    expect(screen.getAllByText("open-control-engine commit every graph is checked against").length).toBeGreaterThan(0);
    fireEvent.pointerDown(chip);
    fireEvent.click(chip);
    expect(screen.queryByText("open-control-engine commit every graph is checked against")).toBeNull();
  });
});
