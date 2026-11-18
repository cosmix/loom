import { describe, expect, it, vi } from "vitest";

import { loadCodeMirror } from "./load";

const chunk = vi.hoisted(() => ({ broken: true }));

// A chunk that fails to load (network drop, stale deploy) until `broken` is cleared.
vi.mock("./codemirror", () => {
  if (chunk.broken) throw new Error("chunk failed to load");
  return { createSourceEditor: vi.fn() };
});

describe("loadCodeMirror after a failed import", () => {
  it("retries on the next call instead of remembering the failure", async () => {
    const first = loadCodeMirror();
    await expect(first).rejects.toThrow();

    chunk.broken = false;
    const second = loadCodeMirror();
    expect(second).not.toBe(first);
    expect((await second).createSourceEditor).toBeTypeOf("function");
    expect(loadCodeMirror()).toBe(second);
  });
});
