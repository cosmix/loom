import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { DRAFT_DEBOUNCE_MS, useDraftWriter } from "./draft-writer";
import { findDrafts, loadDraft, type SaveDraftResult } from "./drafts";

function setup() {
  const results: SaveDraftResult[] = [];
  const hook = renderHook(() => useDraftWriter((result) => results.push(result)));
  return { results, ...hook };
}

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  store.set(serverInfoAtom, {
    project: "example",
    project_key: "example-1234",
    port: 7272,
    remote: false,
    read_only: [],
    stopping: false,
    you: { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" },
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  Reflect.deleteProperty(document, "visibilityState");
  localStorage.clear();
  store.set(serverInfoAtom, null);
});

describe("useDraftWriter", () => {
  it("writes once, with the last text, after typing pauses", () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const { result, results } = setup();
    for (const text of ["a", "ab", "abc"]) result.current.schedule("doc/a.md", "h1", text);
    act(() => void vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS - 1));
    expect(setItem).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(1));
    expect(setItem).toHaveBeenCalledTimes(1);
    expect(loadDraft("doc/a.md", "h1")).toBe("abc");
    expect(results).toEqual(["saved"]);
  });

  it("restarts the pause on every keystroke", () => {
    const { result } = setup();
    result.current.schedule("doc/a.md", "h1", "a");
    act(() => void vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS - 100));
    result.current.schedule("doc/a.md", "h1", "ab");
    act(() => void vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS - 100));
    expect(loadDraft("doc/a.md", "h1")).toBeNull();
    act(() => void vi.advanceTimersByTime(100));
    expect(loadDraft("doc/a.md", "h1")).toBe("ab");
  });

  it("flushes at once when the window loses focus or the page unloads", () => {
    const { result } = setup();
    result.current.schedule("doc/a.md", "h1", "on blur");
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(loadDraft("doc/a.md", "h1")).toBe("on blur");
    result.current.schedule("doc/a.md", "h2", "on unload");
    act(() => void window.dispatchEvent(new Event("beforeunload")));
    expect(loadDraft("doc/a.md", "h2")).toBe("on unload");
  });

  it("flushes when the page becomes hidden but not when it becomes visible", () => {
    const { result } = setup();
    result.current.schedule("doc/a.md", "h1", "pending");
    setVisibility("visible");
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    expect(loadDraft("doc/a.md", "h1")).toBeNull();
    setVisibility("hidden");
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    expect(loadDraft("doc/a.md", "h1")).toBe("pending");
  });

  it("flushes on unmount and stops listening", () => {
    const { result, unmount } = setup();
    result.current.schedule("doc/a.md", "h1", "last words");
    unmount();
    expect(loadDraft("doc/a.md", "h1")).toBe("last words");
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    window.dispatchEvent(new Event("blur"));
    vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS * 2);
    expect(setItem).not.toHaveBeenCalled();
  });

  it("drops a cancelled write for good", () => {
    const { result, unmount } = setup();
    result.current.schedule("doc/a.md", "h1", "discarded");
    result.current.cancel();
    act(() => void vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS * 2));
    unmount();
    expect(findDrafts("doc/a.md")).toEqual([]);
  });

  it("writes the previous document's text before taking a new document's", () => {
    const { result } = setup();
    result.current.schedule("doc/a.md", "h1", "first doc");
    result.current.schedule("doc/b.md", "h9", "second doc");
    expect(loadDraft("doc/a.md", "h1")).toBe("first doc");
    act(() => void vi.advanceTimersByTime(DRAFT_DEBOUNCE_MS));
    expect(loadDraft("doc/b.md", "h9")).toBe("second doc");
  });
});
