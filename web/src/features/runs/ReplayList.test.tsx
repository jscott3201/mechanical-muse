import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ReplayList } from "./ReplayList";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function answer(status: number, body: unknown) {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status })));
}

describe("ReplayList", () => {
  it("says the server has no recorded runs when it answers no-replays", async () => {
    answer(404, { code: "no-replays", message: "this server has no recorded runs to replay" });
    render(<ReplayList highlight={null} onStarted={() => {}} />);
    expect(await screen.findByText(/No recorded runs on this server/)).toBeTruthy();
  });

  it("says the same for an empty list", async () => {
    answer(200, []);
    render(<ReplayList highlight={null} onStarted={() => {}} />);
    expect(await screen.findByText(/No recorded runs on this server/)).toBeTruthy();
  });

  it("shows any other failure with a retry", async () => {
    answer(500, { code: "replay-list", message: "could not read the recordings" });
    render(<ReplayList highlight={null} onStarted={() => {}} />);
    expect(await screen.findByText("could not read the recordings")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });
});
