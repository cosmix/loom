import { expect, it } from "vitest";

import { modelColor } from "@/lib/model-color";

it("modelColor gives the same color for the same name on every call", () => {
  expect(modelColor("claude-opus-5-5")).toBe(modelColor("claude-opus-5-5"));
});

it("modelColor pins a known name so a hash change is caught", () => {
  expect(modelColor("claude-opus-5-5")).toBe("oklch(0.72 0.16 210)");
});

it("modelColor gives distinct names distinct hues", () => {
  const names = ["claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];
  expect(new Set(names.map(modelColor)).size).toBe(names.length);
});

it("modelColor keeps the hue inside 0 to 359", () => {
  for (const name of ["", "a", "gpt-6.1-terra", "gpt-6-luna", "x".repeat(500)]) {
    const hue = Number(modelColor(name).match(/ (\d+)\)$/)?.[1]);
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  }
});
