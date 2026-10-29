import { expect, it } from "vitest";

import { isFeatureName } from "@/plan/new/feature-name";

it.each([
  ["planner-search", true],
  ["a", true],
  ["0", true],
  ["2-plans", true],
  ["feature--", true],
  ["plans-", true],
  ["a".repeat(64), true],
  ["a".repeat(65), false],
  ["", false],
  ["-feature", false],
  ["Feature", false],
  ["feature_name", false],
  ["feature name", false],
  ["feature/name", false],
  ["café", false],
  ["feature\n", false],
  ["feature\r", false],
  ["feature\u202e", false],
  ["archive", false],
  ["briefs", false],
  ["plans", false],
  ["overview", false],
] as const)("validates %j as %s", (name, valid) => {
  expect(isFeatureName(name)).toBe(valid);
});
