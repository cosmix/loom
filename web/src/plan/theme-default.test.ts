import { createStore } from "jotai";
import { afterEach, expect, it } from "vitest";

import { darkVariantAtom } from "@/aurora-ui/shared/atoms/theme";
import { seedPlannerDarkTheme } from "./theme-default";

afterEach(() => localStorage.clear());

it("gives the planner Graphite when no dark theme was picked", () => {
  const store = createStore();
  seedPlannerDarkTheme(store, localStorage);
  expect(store.get(darkVariantAtom)).toBe("gray");
});

it("keeps a dark theme the reader picked", () => {
  localStorage.setItem("loom:dark-variant", JSON.stringify("walnut"));
  const store = createStore();
  seedPlannerDarkTheme(store, localStorage);
  expect(store.get(darkVariantAtom)).toBe("walnut");
});

it("leaves the shared default when storage throws", () => {
  const blocked = {
    getItem: () => {
      throw new Error("blocked");
    },
  } as unknown as Storage;
  const store = createStore();
  seedPlannerDarkTheme(store, blocked);
  expect(store.get(darkVariantAtom)).toBe("walnut");
});
