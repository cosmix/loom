import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import type { DocView } from "@/plan/api/schema-files";
import { docChangedAtom, openDocAtom, openDocIsStaleAtom } from "@/plan/files/atoms";

const doc: DocView = {
  feature: null,
  misplaced: false,
  path: "plans/example.md",
  title: "Example",
  text: "Example text",
  hash: "old",
  kind: "plan",
  editable: true,
  lock: null,
  sections: [],
  plan: null,
  last_change: null,
  too_large: false,
  truncated: false,
  size: 12,
};

describe("openDocIsStaleAtom", () => {
  it("is fresh without an open document or a change for its path", () => {
    const store = createStore();
    store.set(docChangedAtom, { [doc.path]: "new" });
    expect(store.get(openDocIsStaleAtom)).toBe(false);
    store.set(openDocAtom, { doc, seenHash: "old" });
    store.set(docChangedAtom, { "other.md": "new" });
    expect(store.get(openDocIsStaleAtom)).toBe(false);
  });

  it("tracks changed, matching, and deleted hashes against the seen hash", () => {
    const store = createStore();
    store.set(openDocAtom, { doc, seenHash: "old" });
    store.set(docChangedAtom, { [doc.path]: "new" });
    expect(store.get(openDocIsStaleAtom)).toBe(true);
    store.set(openDocAtom, { doc, seenHash: "new" });
    expect(store.get(openDocIsStaleAtom)).toBe(false);
    store.set(docChangedAtom, { [doc.path]: null });
    expect(store.get(openDocIsStaleAtom)).toBe(true);
  });

  it("ignores inherited object properties when no event exists", () => {
    const store = createStore();
    store.set(openDocAtom, { doc: { ...doc, path: "constructor" }, seenHash: "old" });
    expect(store.get(openDocIsStaleAtom)).toBe(false);
  });
});
