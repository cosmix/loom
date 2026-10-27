import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { clearNotesIn, notesAtom, openRunsAtom } from "./view-state";
import type { OpenRun } from "./types";

const run: OpenRun = {
  level: 1,
  sentenceId: "sentence",
  restsOn: [{ path: "doc/product/section_01.md", anchor: "intro", title: "Intro" }],
  parent: null,
};

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

it("keeps the in-memory runs when storage fails", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("blocked");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("blocked");
  });
  const store = createStore();
  const state = openRunsAtom("failure");
  const unsubscribe = store.sub(state, () => {});
  store.set(state, [run]);
  expect(store.get(state)).toEqual([run]);
  unsubscribe();
  const remount = store.sub(state, () => {});
  expect(store.get(state)).toEqual([run]);
  remount();
});

it("drops malformed and invalid stored runs", () => {
  const state = openRunsAtom("invalid");
  for (const value of ["{", JSON.stringify([{ ...run, level: 3 }])]) {
    sessionStorage.setItem("loom-plan:concept:invalid", value);
    const store = createStore();
    const unsubscribe = store.sub(state, () => {});
    expect(store.get(state)).toEqual([]);
    unsubscribe();
  }
});

it("clears only notes in the selected paragraph and page", () => {
  const store = createStore();
  const note = { paragraphOf: "one", text: "folded", path: "doc/a.md" };
  store.set(notesAtom("page"), [note, { ...note, paragraphOf: "two" }]);
  store.set(notesAtom("other"), [note]);
  clearNotesIn(store, "page", "one");
  expect(store.get(notesAtom("page"))).toEqual([{ ...note, paragraphOf: "two" }]);
  expect(store.get(notesAtom("other"))).toEqual([note]);
});

it("restores stored runs in a remounted store", () => {
  const state = openRunsAtom("reload");
  expect(openRunsAtom("reload")).toBe(state);
  expect(notesAtom("reload")).toBe(notesAtom("reload"));
  const first = createStore();
  const unmount = first.sub(state, () => {});
  first.set(state, [run]);
  unmount();
  expect(JSON.parse(sessionStorage.getItem("loom-plan:concept:reload") ?? "null")).toEqual([run]);
  const second = createStore();
  expect(second.get(state)).toEqual([run]);
  second.set(state, (current) => [...current, { ...run, sentenceId: "next" }]);
  expect(second.get(state)).toEqual([run, { ...run, sentenceId: "next" }]);
  const unsubscribe = second.sub(state, () => {});
  expect(second.get(state)).toEqual([run, { ...run, sentenceId: "next" }]);
  unsubscribe();
});
