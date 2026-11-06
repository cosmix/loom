import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { readEditorMode, writeEditorMode } from "./editor-mode";

const KEY = "loom-plan:example-1234:editor-mode";

function setProject(projectKey: string | null) {
  store.set(
    serverInfoAtom,
    projectKey === null
      ? null
      : {
          project: "example",
          project_key: projectKey,
          port: 7272,
          remote: false,
          read_only: [],
          stopping: false,
          you: { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" },
        },
  );
}

beforeEach(() => {
  localStorage.clear();
  setProject("example-1234");
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  setProject(null);
});

describe("editor mode memory", () => {
  it("defaults to live preview", () => {
    expect(readEditorMode()).toBe("preview");
  });

  it("round-trips both modes under the project-scoped key", () => {
    writeEditorMode("source");
    expect(localStorage.getItem(KEY)).toBe("source");
    expect(readEditorMode()).toBe("source");
    writeEditorMode("preview");
    expect(localStorage.getItem(KEY)).toBe("preview");
    expect(readEditorMode()).toBe("preview");
  });

  it("reads an unknown stored value as live preview", () => {
    localStorage.setItem(KEY, "wysiwyg");
    expect(readEditorMode()).toBe("preview");
  });

  it("keeps projects apart", () => {
    writeEditorMode("source");
    setProject("other-5678");
    expect(readEditorMode()).toBe("preview");
  });

  it("reads preview and writes nothing before the project key is known", () => {
    setProject(null);
    writeEditorMode("source");
    expect(localStorage.length).toBe(0);
    localStorage.setItem(KEY, "source");
    expect(readEditorMode()).toBe("preview");
  });

  it("survives blocked storage on read and write", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    expect(readEditorMode()).toBe("preview");
    expect(() => writeEditorMode("source")).not.toThrow();
  });
});
