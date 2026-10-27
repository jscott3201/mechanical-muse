import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiFailure, request } from "./api";

afterEach(() => vi.unstubAllGlobals());

function respond(status: number, body: string) {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(body, { status })));
}

describe("request", () => {
  it("returns the parsed body", async () => {
    respond(200, '{"status":"ok"}');
    await expect(request<{ status: string }>("GET", "/api/health")).resolves.toEqual({ status: "ok" });
  });

  it("keeps the server's error code and message", async () => {
    respond(409, '{"code":"run-active","message":"a run is already active: run-1"}');
    const err = await request("POST", "/api/runs", {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiFailure);
    expect((err as ApiFailure).code).toBe("run-active");
    expect((err as ApiFailure).status).toBe(409);
  });

  it("falls back to the status when the body is not JSON", async () => {
    respond(502, "Bad Gateway");
    const err = (await request("GET", "/api/specs").catch((e: unknown) => e)) as ApiFailure;
    expect(err.code).toBe("http-502");
  });
});
