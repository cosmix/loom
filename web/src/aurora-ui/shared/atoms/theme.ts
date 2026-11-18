// Storage key prefix is "uikit" by default. Change the prefix strings below
// (e.g. "myapp:theme") to namespace the keys for your project. Each atom uses
// a separate key so light-variant and dark-variant preferences persist
// independently from the color-scheme choice.

import { atomWithStorage, createJSONStorage } from "jotai/utils";

export type ColorScheme = "light" | "dark";
export type LightVariant = "ledger" | "gray" | "cool" | "harbor";
export type DarkVariant = "aubergine" | "gray" | "midnight" | "obsidian" | "indigo";

export const ALL_LIGHT_VARIANTS: LightVariant[] = ["ledger", "gray", "cool", "harbor"];
export const ALL_DARK_VARIANTS: DarkVariant[] = [
  "aubergine",
  "gray",
  "midnight",
  "obsidian",
  "indigo",
];

function getInitialColorScheme(): ColorScheme {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export const themeAtom = atomWithStorage<ColorScheme>(
  "loom:theme",
  getInitialColorScheme(),
  undefined,
  { getOnInit: true },
);

export const lightVariantAtom = atomWithStorage<LightVariant>(
  "loom:light-variant",
  "ledger",
  undefined,
  { getOnInit: true },
);

// A stored id that no dark variant carries (a theme since dropped) reads as the default, so the
// root never gets a `data-dark-theme` that no rule styles.
const jsonStorage = createJSONStorage<DarkVariant>();
const darkVariantStorage: typeof jsonStorage = {
  ...jsonStorage,
  getItem: (key, initialValue) => {
    const stored = jsonStorage.getItem(key, initialValue);
    return ALL_DARK_VARIANTS.includes(stored) ? stored : initialValue;
  },
};

export const darkVariantAtom = atomWithStorage<DarkVariant>(
  "loom:dark-variant",
  "aubergine",
  darkVariantStorage,
  { getOnInit: true },
);

// Alias kept for ergonomic parity with the source project.
export const colorSchemeAtom = themeAtom;
