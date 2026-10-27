import type { createStore } from "jotai";
import { z } from "@/api/zod";
import { unauthorizedAtom } from "@/plan/state/atoms";

export type ApiResult<T> =
  | { ok: true; value: T }
  | { ok: false; status: number; error: string; message: string };

let clientStore: ReturnType<typeof createStore> | undefined;

export function initClient(store: ReturnType<typeof createStore>): void {
  clientStore = store;
}

const errorSchema = z.object({ error: z.string(), message: z.string().optional() });

async function request<T>(
  path: string,
  schema: z.ZodType<T>,
  init: RequestInit,
): Promise<ApiResult<T>> {
  let response: Response;
  try {
    response = await fetch(path, { ...init, credentials: "same-origin" });
  } catch (error) {
    return {
      ok: false,
      status: 0,
      error: "unreachable",
      message: error instanceof Error ? error.message : String(error),
    };
  }
  if (response.status === 401) clientStore?.set(unauthorizedAtom, true);
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  if (!response.ok) {
    const parsed = errorSchema.safeParse(body);
    return {
      ok: false,
      status: response.status,
      error: parsed.success ? parsed.data.error : "http_error",
      message: parsed.success
        ? (parsed.data.message ?? parsed.data.error)
        : `HTTP ${response.status}`,
    };
  }
  const parsed = schema.safeParse(body);
  if (!parsed.success) {
    return { ok: false, status: 0, error: "bad_response", message: "Invalid server response" };
  }
  return { ok: true, value: parsed.data };
}

export function apiGet<T>(
  path: string,
  schema: z.ZodType<T>,
  signal?: AbortSignal,
): Promise<ApiResult<T>> {
  return request(path, schema, { method: "GET", signal });
}

export function apiPost<T>(
  path: string,
  body: unknown | string,
  schema: z.ZodType<T>,
  opts: { contentType: "json" | "text" },
): Promise<ApiResult<T>> {
  const json = opts.contentType === "json";
  return request(path, schema, {
    method: "POST",
    headers: { "Content-Type": json ? "application/json" : "text/plain; charset=utf-8" },
    body: json ? JSON.stringify(body) : String(body),
  });
}
