import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Overview } from "./Overview";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Overview", () => {
  it("shows a failed sequence list with a retry, never Loading… forever", async () => {
    let specsUp = false;
    const fetch = vi.fn(async (url: string) => {
      if (url === "/api/specs" && !specsUp)
        return new Response(JSON.stringify({ code: "internal", message: "specs unavailable" }), { status: 500 });
      if (url === "/api/specs") return new Response(JSON.stringify([{ name: "a", title: "A", scenarios: 3 }]), { status: 200 });
      return new Response("[]", { status: 200 });
    });
    vi.stubGlobal("fetch", fetch);
    render(<Overview health={null} onGo={() => {}} />);
    expect(await screen.findByText("specs unavailable")).toBeTruthy();
    expect(screen.queryByText(/Loading…/)).toBeNull();
    specsUp = true;
    act(() => screen.getByRole("button", { name: "Try again" }).click());
    expect(await screen.findByText(/1 sequences/)).toBeTruthy();
  });
});
