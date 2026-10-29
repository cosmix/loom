import { describe, expect, it } from "vitest";

import { chipTitle } from "./document-key";

describe("chipTitle", () => {
  it.each([
    ["01 — Planning", "Planning"],
    ["2 – Review", "Review"],
    ["3 - Release", "Release"],
    ["  04  —  Planning", "Planning"],
    ["Planning", "Planning"],
    ["3-D rendering", "3-D rendering"],
    ["Workflow 2 — Model", "Workflow 2 — Model"],
  ])("formats %j as %j", (title, expected) => {
    expect(chipTitle(title)).toBe(expected);
  });
});
