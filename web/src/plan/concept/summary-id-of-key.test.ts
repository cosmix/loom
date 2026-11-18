import { describe, expect, it } from "vitest";

import { summaryIdOfKey } from "./column-context";

describe("summaryIdOfKey", () => {
  it.each([
    ["demo/engineering/my-plan", "plans/my-plan"],
    ["demo/engineering/feat-x.v2", "plans/feat-x.v2"],
    ["plans/standalone", "plans/standalone"],
    ["overview/demo/product", "overview/demo/product"],
    ["demo/product/section_01", "demo/product/section_01"],
  ])("maps page %s to summary %s", (key, summary) => {
    expect(summaryIdOfKey(key)).toBe(summary);
  });

  it.each([
    "",
    "engineering/my-plan",
    "product/section_01",
    "overview/demo/other",
    "Bad/engineering/my-plan",
    "demo/engineering/",
    "demo/engineering/..",
    "demo/engineering/my/plan",
    "plans/..",
  ])("preserves invalid key %j", (key) => {
    expect(summaryIdOfKey(key)).toBe(key);
  });
});
