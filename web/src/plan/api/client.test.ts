import { createStore } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";
import { z } from "@/api/zod";
import { apiGet, apiPost, initClient } from "@/plan/api/client";
import { unauthorizedAtom } from "@/plan/state/atoms";

const schema = z.object({ value: z.string() });

afterEach(() => vi.unstubAllGlobals());

describe("plan API client", () => {
  it("returns unreachable when fetch rejects for GET and POST", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const expected = { ok: false, status: 0, error: "unreachable", message: "Failed to fetch" };
    expect(await apiGet("/api/server", schema)).toEqual(expected);
    expect(await apiPost("/api/test", {}, schema, { contentType: "json" })).toEqual(expected);
  });

  it("sends credentials, signal, and correctly encoded bodies", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response('{"value":"ok"}'));
    vi.stubGlobal("fetch", fetchMock);
    const signal = new AbortController().signal;
    expect(await apiGet("/api/test", schema, signal)).toEqual({ ok: true, value: { value: "ok" } });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/test", {
      method: "GET",
      signal,
      credentials: "same-origin",
    });
    await apiPost("/api/test", { value: "ok" }, schema, { contentType: "json" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/test", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: '{"value":"ok"}',
    });
    await apiPost("/api/test", "plain text", schema, { contentType: "text" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/test", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "text/plain; charset=utf-8" },
      body: "plain text",
    });
  });

  it("sets unauthorized for either request even with an invalid error body", async () => {
    const store = createStore();
    initClient(store);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(() => Promise.resolve(new Response("bad", { status: 401 }))),
    );
    expect(await apiGet("/api/test", schema)).toMatchObject({ ok: false, status: 401 });
    expect(store.get(unauthorizedAtom)).toBe(true);
    store.set(unauthorizedAtom, false);
    expect(await apiPost("/api/test", {}, schema, { contentType: "json" })).toMatchObject({
      ok: false,
      status: 401,
    });
    expect(store.get(unauthorizedAtom)).toBe(true);
  });

  it("returns server errors and rejects malformed successful responses", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response('{"error":"refused","message":"No access"}', { status: 403 }),
      )
      .mockResolvedValueOnce(new Response('{"value":42}'))
      .mockResolvedValueOnce(new Response("not json"));
    vi.stubGlobal("fetch", fetchMock);
    expect(await apiGet("/api/test", schema)).toEqual({
      ok: false,
      status: 403,
      error: "refused",
      message: "No access",
    });
    expect(await apiGet("/api/test", schema)).toMatchObject({
      ok: false,
      status: 0,
      error: "bad_response",
    });
    expect(await apiGet("/api/test", schema)).toMatchObject({
      ok: false,
      status: 0,
      error: "bad_response",
    });
  });

  it("parses the body of a listed result status and leaves other statuses as errors", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response('{"value":"stale"}', { status: 409 }))
      .mockResolvedValueOnce(new Response('{"value":42}', { status: 409 }))
      .mockResolvedValueOnce(new Response('{"value":"stale"}', { status: 409 }))
      .mockResolvedValueOnce(new Response('{"value":"x"}', { status: 500 }));
    vi.stubGlobal("fetch", fetchMock);
    const listed = { contentType: "text", resultStatuses: [409, 422] } as const;
    expect(await apiPost("/api/test", "t", schema, listed)).toEqual({
      ok: true,
      value: { value: "stale" },
    });
    expect(await apiPost("/api/test", "t", schema, listed)).toMatchObject({
      ok: false,
      error: "bad_response",
    });
    expect(await apiPost("/api/test", "t", schema, { contentType: "text" })).toMatchObject({
      ok: false,
      status: 409,
      error: "http_error",
    });
    expect(await apiPost("/api/test", "t", schema, listed)).toMatchObject({
      ok: false,
      status: 500,
    });
  });
});
