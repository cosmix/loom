// Storage key prefix is "uikit" by default. Change the prefix strings below
// (e.g. "myapp:theme") to namespace the keys for your project. Each atom uses
// a separate key so light-variant and dark-variant preferences persist
// independently from the color-scheme choice.

import { atomWithStorage } from "jotai/utils";

export type ColorScheme = "light" | "dark";
export type LightVariant = "default" | "gray" | "cool";
export type DarkVariant =
  | "purple"
  | "green"
  | "gray"
  | "blue"
  | "slate"
  | "sand";

export const ALL_LIGHT_VARIANTS: LightVariant[] = ["default", "gray", "cool"];
export const ALL_DARK_VARIANTS: DarkVariant[] = [
  "purple",
  "green",
  "gray",
  "blue",
  "slate",
  "sand",
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
  "default",
  undefined,
  { getOnInit: true },
);

export const darkVariantAtom = atomWithStorage<DarkVariant>(
  "loom:dark-variant",
  "purple",
  undefined,
  { getOnInit: true },
);

// Alias kept for ergonomic parity with the source project.
export const colorSchemeAtom = themeAtom;
