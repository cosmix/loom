import { createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const KEY = "loom:dark-variant";

// The atom reads storage once when the module loads, so each case loads a fresh copy.
async function darkVariantFromStorage(stored: string): Promise<string> {
  localStorage.setItem(KEY, stored);
  vi.resetModules();
  const { darkVariantAtom } = await import("./theme");
  return createStore().get(darkVariantAtom);
}

beforeEach(() => localStorage.clear());
afterEach(() => localStorage.clear());

it("keeps a stored dark variant that exists", async () => {
  expect(await darkVariantFromStorage(JSON.stringify("gray"))).toBe("gray");
});

it("reads a stored id no dark variant carries as the default", async () => {
  expect(await darkVariantFromStorage(JSON.stringify("walnut"))).toBe("aubergine");
});
