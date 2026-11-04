import { expect, it } from "vitest";

import { passageHref } from "./passage";

const LINES = { start: 4, end: 11 };

it("a feature section's passage opens its source inside the feature", () => {
  const anchor = { path: "doc/product/atlas/section_01.md", anchor: "the-intent" };
  expect(passageHref(anchor, LINES)).toBe(
    "/f/atlas/source/product/atlas/section_01.md?lines=4-11#the-intent",
  );
});

it("a feature plan's passage opens its source inside the feature", () => {
  const anchor = { path: "doc/plans/atlas/PLAN-x.md", anchor: "stages" };
  expect(passageHref(anchor, null)).toBe("/f/atlas/source/plans/atlas/PLAN-x.md#stages");
});

it("a standalone plan's passage keeps file mode", () => {
  const anchor = { path: "doc/plans/PLAN-x.md", anchor: "stages" };
  expect(passageHref(anchor, LINES)).toBe("/files/plans/PLAN-x.md?lines=4-11#stages");
});
