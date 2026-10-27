import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import { pendingEditFor, requestEdit, requestEditAtom } from "./request-edit";

describe("requestEdit", () => {
  it("stores the request and returns an encoded edit address", () => {
    const store = createStore();
    const req = { path: "doc/plans/a #1.md", text: "restored text", openedHash: "old-hash" };

    expect(store.get(requestEditAtom)).toBeNull();
    expect(requestEdit(store, req)).toBe("/files/plans/a%20%231.md?edit=1");
    expect(store.get(requestEditAtom)).toEqual(req);
  });

  it("replaces a pending request and can be consumed", () => {
    const store = createStore();
    requestEdit(store, { path: "first.md", text: "first", openedHash: "one" });
    const next = { path: "second.md", text: "", openedHash: "two" };

    expect(requestEdit(store, next)).toBe("/files/second.md?edit=1");
    expect(store.get(requestEditAtom)).toEqual(next);
    store.set(requestEditAtom, null);
    expect(store.get(requestEditAtom)).toBeNull();
  });
});

describe("pendingEditFor", () => {
  it("finds a request named doc-relative or repo-relative for either form of the path", () => {
    const store = createStore();
    requestEdit(store, { path: "product/section_01.md", text: "kept", openedHash: "old" });
    expect(pendingEditFor(store, "doc/product/section_01.md")?.text).toBe("kept");
    expect(pendingEditFor(store, "product/section_01.md")?.text).toBe("kept");

    requestEdit(store, { path: "doc/product/section_01.md", text: "again", openedHash: "old" });
    expect(pendingEditFor(store, "product/section_01.md")?.text).toBe("again");
    expect(pendingEditFor(store, "doc/product/section_01.md")?.text).toBe("again");
  });

  it("ignores a request for another document and an empty atom", () => {
    const store = createStore();
    expect(pendingEditFor(store, "doc/a.md")).toBeNull();
    requestEdit(store, { path: "b.md", text: "x", openedHash: "h" });
    expect(pendingEditFor(store, "doc/a.md")).toBeNull();
  });
});
