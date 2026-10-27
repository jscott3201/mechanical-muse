import type { ApiError } from "@/types/generated/ApiError";

/** A refused or failed request. Branch on `code` (stable); show `message`. */
export class ApiFailure extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiFailure";
    this.status = status;
    this.code = code;
  }
}

function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as Record<string, unknown>).code === "string" &&
    typeof (value as Record<string, unknown>).message === "string"
  );
}

/**
 * Calls mm-server. The response type comes from the generated DTOs; the body is trusted to
 * match because the server is the only producer (the Rust DTO owns the contract).
 */
export async function request<T>(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const init: RequestInit = { method };
  if (signal) init.signal = signal;
  if (body !== undefined) {
    init.headers = { "content-type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const response = await fetch(path, init);
  const text = await response.text();
  if (!response.ok) {
    let parsed: unknown = null;
    try {
      parsed = JSON.parse(text);
    } catch {
      // Not JSON (a proxy or network error page): fall through to the status text.
    }
    if (isApiError(parsed)) throw new ApiFailure(response.status, parsed.code, parsed.message);
    throw new ApiFailure(
      response.status,
      `http-${response.status}`,
      response.statusText || "request failed",
    );
  }
  return JSON.parse(text) as T;
}
