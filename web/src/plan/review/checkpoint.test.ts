import { createStore, getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema } from "@/plan/api/schema";
import { storageFullAtom, storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import { changedSince, checkpointAtom, markSeen, type Checkpoint } from "./checkpoint";

const hello = planMessageSchema.parse(helloJson);
if (hello.type !== "hello") throw new Error("hello.json is not a hello frame");
const server = hello.server;

function readyStore(projectKey: string) {
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  return store;
}

beforeEach(() => localStorage.clear());
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  getDefaultStore().set(storageFullAtom, false);
});

it("stores checkpoints per project key and marks only the selected path seen", () => {
  const first: Checkpoint = {
    commit: "c1",
    hashes: { "doc/a.md": "a1", "doc/b.md": "b1" },
  };
  const second: Checkpoint = { commit: null, hashes: { "doc/a.md": "other" } };
  const store = readyStore("first");
  expect(store.get(checkpointAtom)).toBeNull();
  expect(localStorage.getItem(storageKey("first", "checkpoint"))).toBeNull();
  store.set(checkpointAtom, first);
  store.set(serverInfoAtom, { ...server, project_key: "second" });
  expect(store.get(checkpointAtom)).toBeNull();
  store.set(checkpointAtom, second);
  store.set(serverInfoAtom, { ...server, project_key: "first" });

  markSeen(store, "doc/a.md", "a2");

  const expected = { commit: "c1", hashes: { "doc/a.md": "a2", "doc/b.md": "b1" } };
  expect(store.get(checkpointAtom)).toEqual(expected);
  expect(JSON.parse(localStorage.getItem(storageKey("first", "checkpoint"))!)).toEqual(expected);
  expect(readyStore("first").get(checkpointAtom)).toEqual(expected);
  expect(readyStore("second").get(checkpointAtom)).toEqual(second);
  expect(first.hashes["doc/a.md"]).toBe("a1");
});

it("lists a removed document even when there are no current documents", () => {
  const checkpoint: Checkpoint = { commit: null, hashes: { "doc/deleted.md": "old" } };

  expect(changedSince(checkpoint, {})).toEqual([{ path: "doc/deleted.md", change: "removed" }]);
});

it("compares hashes purely, retaining unchanged documents in both inputs", () => {
  const checkpoint = Object.freeze({
    commit: "c1",
    hashes: Object.freeze({ "doc/changed.md": "old", "doc/same.md": "same" }),
  });
  const current = Object.freeze({
    "doc/added.md": "new",
    "doc/changed.md": "new",
    "doc/same.md": "same",
    toString: "own hash",
  });

  expect(changedSince(checkpoint, current)).toEqual([
    { path: "doc/added.md", change: "added" },
    { path: "doc/changed.md", change: "changed" },
    { path: "toString", change: "added" },
  ]);
  expect(changedSince(checkpoint, checkpoint.hashes)).toEqual([]);
  expect(changedSince({ commit: null, hashes: {} }, {})).toEqual([]);
});

it.each([
  "{",
  "null",
  JSON.stringify({ hashes: {} }),
  JSON.stringify({ commit: 12, hashes: {} }),
  JSON.stringify({ commit: null, hashes: { "doc/a.md": 12 } }),
])("ignores invalid stored checkpoints: %s", (stored) => {
  localStorage.setItem(storageKey("invalid", "checkpoint"), stored);

  expect(readyStore("invalid").get(checkpointAtom)).toBeNull();
});

it("keeps updates in memory when storage is unavailable", () => {
  const store = readyStore("blocked");
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new DOMException("Blocked", "SecurityError");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Full", "QuotaExceededError");
  });
  expect(store.get(checkpointAtom)).toBeNull();
  store.set(checkpointAtom, { commit: "c1", hashes: { "doc/a.md": "old" } });

  markSeen(store, "doc/a.md", "seen");

  expect(store.get(checkpointAtom)).toEqual({
    commit: "c1",
    hashes: { "doc/a.md": "seen" },
  });
  expect(getDefaultStore().get(storageFullAtom)).toBe(true);
});

it("leaves missing checkpoints and unknown projects uninitialized", () => {
  const store = createStore();
  store.set(checkpointAtom, { commit: "c1", hashes: {} });
  expect(store.get(checkpointAtom)).toBeNull();
  store.set(serverInfoAtom, { ...server, project_key: "first-visit" });

  markSeen(store, "doc/a.md", "seen");

  expect(store.get(checkpointAtom)).toBeNull();
  expect(localStorage.length).toBe(0);
});
