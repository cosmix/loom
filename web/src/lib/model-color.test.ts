import { expect, it } from "vitest";

import {
  assignSwatches,
  distance,
  modelColor,
  preferredSwatch,
  resetModelColors,
  spread,
  stickyModelColors,
} from "@/lib/model-color";

it("modelColor gives the same color for the same name on every call", () => {
  expect(modelColor("claude-opus-5-5")).toBe(modelColor("claude-opus-5-5"));
});

it("modelColor pins a known name so a hash change is caught", () => {
  expect(modelColor("claude-opus-5-5")).toBe("oklch(0.72 0.16 210)");
});

it("modelColor gives distinct names distinct hues", () => {
  const names = [
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
  ];
  expect(new Set(names.map(modelColor)).size).toBe(names.length);
});

it("modelColor keeps the hue inside 0 to 359", () => {
  for (const name of [
    "",
    "a",
    "gpt-6.1-terra",
    "gpt-6-luna",
    "x".repeat(500),
  ]) {
    const hue = Number(modelColor(name).match(/ (\d+)\)$/)?.[1]);
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  }
});

const TARGET_GAP = 0.18;
const FAMILY = [
  "claude-fable-5-1",
  "claude-opus-5-5",
  "claude-sonnet-5-5",
  "claude-haiku-5-5",
];

it("assignSwatches keeps every pair of a model family at least the minimum distance apart", () => {
  const swatches = [...assignSwatches(FAMILY, new Map()).values()];
  for (let i = 0; i < swatches.length; i++) {
    for (let j = i + 1; j < swatches.length; j++) {
      expect(distance(swatches[i]!, swatches[j]!)).toBeGreaterThanOrEqual(
        TARGET_GAP,
      );
    }
  }
});

it("modelColor hashes the whole name, so two versions of one model get different colors", () => {
  expect(modelColor("claude-opus-5-4")).not.toBe(modelColor("claude-opus-5-5"));
  expect(modelColor("gpt-6.1-terra")).not.toBe(modelColor("gpt-6-terra"));
});

it("assignSwatches separates gpt-6.1-sol from claude-opus-5-5", () => {
  const swatches = assignSwatches(
    ["gpt-6.1-sol", "claude-opus-5-5"],
    new Map(),
  );
  expect(
    distance(swatches.get("gpt-6.1-sol")!, swatches.get("claude-opus-5-5")!),
  ).toBeGreaterThanOrEqual(TARGET_GAP);
});

it("assignSwatches keeps a mixed fleet of models at least the target gap apart", () => {
  const fleet = [
    ...FAMILY,
    "gpt-6.1-sol",
    "gpt-6.1-terra",
    "gpt-6-luna",
    "claude-opus-5-4",
    "gemini-3-pro",
  ];
  const swatches = [...assignSwatches(fleet, new Map()).values()];
  for (let i = 0; i < swatches.length; i++) {
    for (let j = i + 1; j < swatches.length; j++) {
      expect(distance(swatches[i]!, swatches[j]!)).toBeGreaterThanOrEqual(
        TARGET_GAP,
      );
    }
  }
});

it("assignSwatches nudges a model off a swatch it would otherwise sit on", () => {
  const taken = new Map([["other", preferredSwatch("claude-opus-5-5")]]);
  const assigned = assignSwatches(["other", "claude-opus-5-5"], taken).get(
    "claude-opus-5-5",
  )!;
  expect(distance(assigned, taken.get("other")!)).toBeGreaterThanOrEqual(
    TARGET_GAP,
  );
});

it("assignSwatches never recolors a model that already has a swatch", () => {
  const first = assignSwatches(FAMILY.slice(0, 2), new Map());
  const later = assignSwatches(FAMILY, first);
  for (const model of FAMILY.slice(0, 2))
    expect(later.get(model)).toEqual(first.get(model));
});

it("assignSwatches is independent of the order models arrive in", () => {
  const forward = assignSwatches(FAMILY, new Map());
  const reversed = assignSwatches([...FAMILY].reverse(), new Map());
  for (const model of FAMILY)
    expect(reversed.get(model)).toEqual(forward.get(model));
});

it("spread never ends up closer to a neighbour than the preferred swatch was when the palette is full", () => {
  const crowded = [0.6, 0.72, 0.84].flatMap((lightness) =>
    Array.from({ length: 24 }, (_, index) => ({ hue: index * 15, lightness })),
  );
  const preferred = { hue: 7, lightness: 0.72 };
  const gap = (swatch: typeof preferred) =>
    Math.min(...crowded.map((other) => distance(swatch, other)));
  expect(gap(spread(preferred, crowded))).toBeGreaterThanOrEqual(
    gap(preferred),
  );
});

it("stickyModelColors keeps a color across calls and survives a reload from storage", () => {
  resetModelColors();
  const first = stickyModelColors(["claude-opus-5-5"]).get("claude-opus-5-5");
  stickyModelColors(["claude-opus-5-5", "claude-haiku-5-5"]);
  expect(stickyModelColors(["claude-opus-5-5"]).get("claude-opus-5-5")).toBe(
    first,
  );
  const stored = localStorage.getItem("loom.model-colors.v2");
  resetModelColors();
  localStorage.setItem("loom.model-colors.v2", stored!);
  expect(stickyModelColors(["claude-opus-5-5"]).get("claude-opus-5-5")).toBe(
    first,
  );
});
