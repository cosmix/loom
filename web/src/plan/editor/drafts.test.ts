import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";
import { clearDrafts, findDrafts, loadDraft, saveDraft } from "./drafts";

function useProject(project_key: string) {
  store.set(serverInfoAtom, {
    project: project_key,
    project_key,
    port: 7272,
    remote: false,
    read_only: [],
    stopping: false,
    you: { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" },
  });
}

beforeEach(() => {
  localStorage.clear();
  useProject("first");
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  store.set(serverInfoAtom, null);
});

describe("draft storage", () => {
  it("isolates projects, paths and hashes, lists newest first, and clears only the path", () => {
    const now = vi.spyOn(Date, "now");
    now.mockReturnValue(1);
    expect(saveDraft("a.md", "h1", "one")).toBe("saved");
    now.mockReturnValue(2);
    expect(saveDraft("a.md", "h2", "two")).toBe("saved");
    saveDraft("a.md:other", "h1", "other path");
    useProject("second");
    saveDraft("a.md", "h1", "other project");
    expect(loadDraft("a.md", "h1")).toBe("other project");
    useProject("first");
    expect(findDrafts("a.md")).toEqual([
      { openedHash: "h2", text: "two", savedAt: 2 },
      { openedHash: "h1", text: "one", savedAt: 1 },
    ]);
    clearDrafts("a.md");
    expect(loadDraft("a.md", "h1")).toBeNull();
    expect(loadDraft("a.md:other", "h1")).toBe("other path");
    useProject("second");
    expect(loadDraft("a.md", "h1")).toBe("other project");
  });

  it("handles unavailable storage and missing project information", () => {
    store.set(serverInfoAtom, null);
    expect(saveDraft("a", "h", "text")).toBe("unavailable");
    useProject("first");
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    localStorage.setItem("loom-plan-draft:broken", "x");
    expect(loadDraft("a", "h")).toBeNull();
    expect(findDrafts("a")).toEqual([]);
    expect(saveDraft("a", "h", "text")).toBe("unavailable");
    expect(() => clearDrafts("a")).not.toThrow();
  });

  it("enforces byte limits and evicts the oldest drafts across projects", () => {
    const now = vi.spyOn(Date, "now");
    now.mockReturnValue(1);
    saveDraft("a", "h", "a".repeat(1024 * 1024));
    now.mockReturnValue(2);
    useProject("second");
    saveDraft("b", "h", "b".repeat(1024 * 1024));
    now.mockReturnValue(3);
    expect(saveDraft("c", "h", "é")).toBe("saved");
    useProject("first");
    expect(loadDraft("a", "h")).toBeNull();
    useProject("second");
    expect(loadDraft("b", "h")).not.toBeNull();
    expect(saveDraft("c", "h", "é".repeat(524289))).toBe("too_large");
    expect(loadDraft("c", "h")).toBe("é");
  });

  it("evicts oldest drafts on quota errors without touching other keys", () => {
    const now = vi.spyOn(Date, "now");
    now.mockReturnValue(1);
    saveDraft("old", "h", "old");
    now.mockReturnValue(2);
    saveDraft("new", "h", "new");
    localStorage.setItem("other-module", "keep");
    const original = Storage.prototype.setItem;
    let failures = 1;
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(function (
      this: Storage,
      key: string,
      value: string,
    ) {
      if (failures-- > 0) throw new DOMException("full", "QuotaExceededError");
      return original.call(this, key, value);
    });
    expect(saveDraft("target", "h", "target")).toBe("saved");
    expect(loadDraft("old", "h")).toBeNull();
    expect(loadDraft("new", "h")).toBe("new");
    expect(localStorage.getItem("other-module")).toBe("keep");
    failures = 10;
    expect(saveDraft("last", "h", "last")).toBe("storage_full");
    expect(localStorage.getItem("other-module")).toBe("keep");
  });
});
