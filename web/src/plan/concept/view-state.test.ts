import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import { clearNotesIn, notesAtom, openRunsAtom } from "./view-state";
import type { OpenRun } from "./types";

const run: OpenRun = {
  level: 1,
  sentenceId: "sentence",
  restsOn: [{ path: "doc/product/section_01.md", anchor: "intro", title: "Intro" }],
  parent: null,
};

const server = serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] });

/** A store that has had its hello, so the page's runs are kept under this project's key. */
function helloStore(projectKey = server.project_key) {
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  return store;
}

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

it("keeps the in-memory runs when storage fails", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("blocked");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("blocked");
  });
  const store = helloStore();
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
    sessionStorage.setItem(storageKey(server.project_key, "concept:invalid"), value);
    const store = helloStore();
    const unsubscribe = store.sub(state, () => {});
    expect(store.get(state)).toEqual([]);
    unsubscribe();
  }
});

it("restores a page's runs only for the project that stored them", () => {
  const state = openRunsAtom("shared-name");
  helloStore("project-a").set(state, [run]);
  const stored = sessionStorage.getItem(storageKey("project-a", "concept:shared-name"));
  expect(JSON.parse(stored ?? "null")).toEqual([run]);
  // A tab reused for another project must not restore the first project's page of the same name.
  expect(helloStore("project-b").get(state)).toEqual([]);
  expect(helloStore("project-a").get(state)).toEqual([run]);
});

it("keeps a page's runs under a shared unkeyed segment until the project key is known", () => {
  const state = openRunsAtom("early");
  const store = createStore();
  store.set(state, [run]);
  const stored = sessionStorage.getItem(storageKey("unkeyed", "concept:early"));
  expect(JSON.parse(stored ?? "null")).toEqual([run]);
  expect(helloStore("project-a").get(state)).toEqual([]);
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
  const first = helloStore();
  const unmount = first.sub(state, () => {});
  first.set(state, [run]);
  unmount();
  const key = storageKey(server.project_key, "concept:reload");
  expect(JSON.parse(sessionStorage.getItem(key) ?? "null")).toEqual([run]);
  const second = helloStore();
  expect(second.get(state)).toEqual([run]);
  second.set(state, (current) => [...current, { ...run, sentenceId: "next" }]);
  expect(second.get(state)).toEqual([run, { ...run, sentenceId: "next" }]);
  const unsubscribe = second.sub(state, () => {});
  expect(second.get(state)).toEqual([run, { ...run, sentenceId: "next" }]);
  unsubscribe();
});
