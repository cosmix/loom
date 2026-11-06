// Storage key prefix is "uikit" by default. Change the prefix strings below
// (e.g. "myapp:theme") to namespace the keys for your project. Each atom uses
// a separate key so light-variant and dark-variant preferences persist
// independently from the color-scheme choice.

import { atomWithStorage } from "jotai/utils";

export type ColorScheme = "light" | "dark";
export type LightVariant = "ledger" | "gray" | "cool";
export type DarkVariant = "aubergine" | "walnut" | "gray";

export const ALL_LIGHT_VARIANTS: LightVariant[] = ["ledger", "gray", "cool"];
export const ALL_DARK_VARIANTS: DarkVariant[] = ["aubergine", "walnut", "gray"];

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

export const darkVariantAtom = atomWithStorage<DarkVariant>(
  "loom:dark-variant",
  "aubergine",
  undefined,
  { getOnInit: true },
);

// Alias kept for ergonomic parity with the source project.
export const colorSchemeAtom = themeAtom;
