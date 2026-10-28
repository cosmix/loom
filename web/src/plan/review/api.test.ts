import { afterEach, describe, expect, it, vi } from "vitest";
import restore from "@/plan/api/fixtures/review-restore.json";
import compare from "@/plan/api/fixtures/review-compare.json";
import suggestion from "@/plan/api/fixtures/review-suggestion.json";
import { reviewMessageSchema } from "@/plan/api/schema-review";
import {
  compareResponseSchema,
  getAtCommit,
  getBuilt,
  getCompare,
  getHashes,
  getImplementedBy,
  getOpenQuestions,
  getTurnSections,
  postRestore,
  resolveRef,
  restoreResponseSchema,
} from "./api";

afterEach(() => vi.unstubAllGlobals());

describe("review wire schemas", () => {
  it("parses the server fixtures without losing fields", () => {
    expect(restoreResponseSchema.parse(restore)).toEqual(restore);
    expect(compareResponseSchema.parse(compare)).toEqual(compare);
    expect(suggestion.map((message) => reviewMessageSchema.parse(message))).toEqual(suggestion);
  });

  it("rejects incomplete variants", () => {
    expect(
      restoreResponseSchema.safeParse({ files: [{ path: "doc/a", result: "changed" }] }).success,
    ).toBe(false);
    expect(
      reviewMessageSchema.safeParse({ kind: "suggestion_status", id: "x", state: "sent" }).success,
    ).toBe(false);
  });
});

describe("review routes", () => {
  it("sends the restore body as JSON", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(restore)));
    vi.stubGlobal("fetch", fetchMock);
    const body = { session: "s", turn: "t", paths: ["doc/a"], force: false };
    expect(await postRestore(body)).toEqual({ ok: true, value: restore });
    expect(fetchMock).toHaveBeenCalledWith("/api/restore", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  });

  it("encodes each route's query parameters", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}"));
    vi.stubGlobal("fetch", fetchMock);
    await getTurnSections("session & one", "turn/one");
    await getHashes();
    await resolveRef("feature/a & b");
    await getCompare("commit & one");
    await getAtCommit("commit & one", "doc/a & b.md");
    await getBuilt("doc/product/a & b.md");
    await getOpenQuestions();
    await getImplementedBy();
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/turn-sections?session=session%20%26%20one&turn=turn%2Fone",
      "/api/hashes",
      "/api/ref?name=feature%2Fa%20%26%20b",
      "/api/compare?commit=commit%20%26%20one",
      "/api/at-commit?commit=commit%20%26%20one&path=doc%2Fa%20%26%20b.md",
      "/api/built?section=product%2Fa%20%26%20b.md",
      "/api/open-questions",
      "/api/implemented-by",
    ]);
    expect(fetchMock).toHaveBeenCalledWith(expect.any(String), {
      method: "GET",
      signal: undefined,
      credentials: "same-origin",
    });
  });
});
