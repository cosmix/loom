import { getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { safeStorage, storageFullAtom, storageKey } from "./storage";

const store = getDefaultStore();
const key = storageKey("example-1234", "draft");

beforeEach(() => {
  localStorage.clear();
  store.set(storageFullAtom, false);
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  store.set(storageFullAtom, false);
});

it("sets storageFull when a quota error prevents a write", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Storage is full", "QuotaExceededError");
  });

  expect(() => safeStorage.setItem(key, "unsaved draft")).not.toThrow();

  expect(store.get(storageFullAtom)).toBe(true);
});

it.each([
  ["a QuotaExceededError", new DOMException("Full", "QuotaExceededError")],
  ["a Firefox quota error", new DOMException("Full", "NS_ERROR_DOM_QUOTA_REACHED")],
])("sets storageFull for %s", (_label, error) => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw error;
  });

  safeStorage.setItem(key, "unsaved draft");

  expect(store.get(storageFullAtom)).toBe(true);
});

it("keeps the value in memory without warning for a non-quota error", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Storage is blocked", "SecurityError");
  });

  expect(() => safeStorage.setItem(key, "unsaved draft")).not.toThrow();

  expect(store.get(storageFullAtom)).toBe(false);
});

it("clears storageFull after a successful write following a quota error", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementationOnce(() => {
    throw new DOMException("Storage is full", "QuotaExceededError");
  });
  safeStorage.setItem(key, "unsaved draft");
  expect(store.get(storageFullAtom)).toBe(true);

  safeStorage.setItem(key, { text: "saved draft" });

  expect(store.get(storageFullAtom)).toBe(false);
  expect(localStorage.getItem(key)).toBe('{"text":"saved draft"}');
  expect(safeStorage.getItem(key, { text: "" })).toEqual({ text: "saved draft" });
});

it("returns the initial value when reading storage throws", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new DOMException("Storage is blocked", "SecurityError");
  });
  const initialValue = { text: "initial draft" };

  expect(safeStorage.getItem(key, initialValue)).toBe(initialValue);
});

it("scopes storage keys to the project key and value name", () => {
  expect(storageKey("example-1234", "draft")).toBe("loom-plan:example-1234:draft");
  expect(storageKey("other-5678", "draft")).toBe("loom-plan:other-5678:draft");
  expect(storageKey("example-1234", "checkpoint")).toBe("loom-plan:example-1234:checkpoint");
});
