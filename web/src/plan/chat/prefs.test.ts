import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema } from "@/plan/api/schema";
import { serverInfoAtom } from "@/plan/state/atoms";
import { holdNoteSeenAtom, panelCollapsedAtom, panelWidthAtom } from "./prefs";
import { storageKey } from "./storage";

const hello = planMessageSchema.parse(helloJson);
if (hello.type !== "hello") throw new Error("hello.json is not a hello frame");
const server = hello.server;

function readyStore(projectKey = "prefs-test") {
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  return store;
}

beforeEach(() => localStorage.clear());
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

it("persists both preferences per project", () => {
  const store = readyStore();
  expect(store.get(panelWidthAtom)).toBe(26);
  expect(store.get(panelCollapsedAtom)).toBe(false);
  store.set(panelWidthAtom, 32);
  store.set(panelCollapsedAtom, true);
  expect(localStorage.getItem(storageKey("prefs-test", "panel-width"))).toBe("32");
  expect(localStorage.getItem(storageKey("prefs-test", "panel-collapsed"))).toBe("true");
  const reloaded = readyStore();
  const unsubWidth = reloaded.sub(panelWidthAtom, () => {});
  const unsubCollapsed = reloaded.sub(panelCollapsedAtom, () => {});
  expect(reloaded.get(panelWidthAtom)).toBe(32);
  expect(reloaded.get(panelCollapsedAtom)).toBe(true);
  unsubWidth();
  unsubCollapsed();
  expect(readyStore("another-project").get(panelWidthAtom)).toBe(26);
});

it("clamps width updates to 20–40 rem", () => {
  const store = readyStore();
  store.set(panelWidthAtom, 10);
  expect(store.get(panelWidthAtom)).toBe(20);
  store.set(panelWidthAtom, (width) => width + 100);
  expect(store.get(panelWidthAtom)).toBe(40);
});

it("keeps preferences in memory when storage writes fail", () => {
  const store = readyStore();
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Storage is full", "QuotaExceededError");
  });
  store.set(panelWidthAtom, 30);
  store.set(panelCollapsedAtom, true);
  expect(store.get(panelWidthAtom)).toBe(30);
  expect(store.get(panelCollapsedAtom)).toBe(true);
});

it.each([
  ["a width above the range", "1000", 40],
  ["a width below the range", "3", 20],
  ["a width in the range", "30", 30],
  ["a string width", '"wide"', 26],
  ["invalid JSON", "{", 26],
  ["null", "null", 26],
])("reads %s (stored %s) as %d", (_label, stored, expected) => {
  localStorage.setItem(storageKey("prefs-bad-width", "panel-width"), stored);
  const store = readyStore("prefs-bad-width");
  store.sub(panelWidthAtom, () => {});
  expect(store.get(panelWidthAtom)).toBe(expected);
});

it.each(['"false"', "1", "null", "{"])("reads collapsed %s from storage as false", (stored) => {
  localStorage.setItem(storageKey("prefs-bad-collapsed", "panel-collapsed"), stored);
  const store = readyStore("prefs-bad-collapsed");
  store.sub(panelCollapsedAtom, () => {});
  expect(store.get(panelCollapsedAtom)).toBe(false);
});

it("keeps the hold note flag per project and validates what it reads", () => {
  const store = readyStore("prefs-hold");
  store.sub(holdNoteSeenAtom, () => {});
  expect(store.get(holdNoteSeenAtom)).toBe(false);
  store.set(holdNoteSeenAtom, true);
  expect(localStorage.getItem(storageKey("prefs-hold", "hold-note-seen"))).toBe("true");
  expect(readyStore("prefs-other").get(holdNoteSeenAtom)).toBe(false);

  localStorage.setItem(storageKey("prefs-hold-bad", "hold-note-seen"), '"yes"');
  const bad = readyStore("prefs-hold-bad");
  bad.sub(holdNoteSeenAtom, () => {});
  expect(bad.get(holdNoteSeenAtom)).toBe(false);
});

it("keeps preferences in memory only before the first hello", () => {
  const store = createStore();
  store.sub(panelWidthAtom, () => {});
  store.set(panelWidthAtom, 33);
  store.set(holdNoteSeenAtom, true);
  expect(store.get(panelWidthAtom)).toBe(33);
  expect(store.get(holdNoteSeenAtom)).toBe(true);
  expect(localStorage.length).toBe(0);
});
