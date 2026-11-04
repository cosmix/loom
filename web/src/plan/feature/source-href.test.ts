import { describe, expect, it } from "vitest";

import { featureOfDocPath, featureSourceHref, sourceHref } from "./source-href";

describe("featureOfDocPath", () => {
  it.each([
    ["doc/product/atlas/index.md", "atlas"],
    ["doc/product/atlas/section_01.md", "atlas"],
    ["doc/plans/atlas/index.md", "atlas"],
    ["doc/plans/atlas/DONE-PLAN-x.md", "atlas"],
    ["product/atlas/section_01.md", "atlas"],
  ])("reads the feature of %s", (path, feature) => {
    expect(featureOfDocPath(path)).toBe(feature);
  });

  it.each([
    "doc/plans/PLAN-x.md",
    "doc/product/section_02.md",
    "doc/plans/archive/PLAN-x.md",
    "doc/knowledge/atlas/x.md",
    "doc/product/Atlas/x.md",
    "doc/product/atlas/../other/x.md",
    "doc/product/atlas//x.md",
    "doc/product/atlas",
  ])("gives no feature for %s", (path) => {
    expect(featureOfDocPath(path)).toBeNull();
  });
});

describe("featureSourceHref", () => {
  it("drops doc/ and encodes each segment", () => {
    expect(featureSourceHref("atlas", "doc/plans/atlas/PLAN #1.md")).toBe(
      "/f/atlas/source/plans/atlas/PLAN%20%231.md",
    );
  });

  it("carries the lines and the anchor", () => {
    expect(
      featureSourceHref("atlas", "doc/product/atlas/section_01.md", {
        lines: { start: 3, end: 9 },
        anchor: "the intent",
      }),
    ).toBe("/f/atlas/source/product/atlas/section_01.md?lines=3-9#the%20intent");
  });

  it("omits a null lines and anchor", () => {
    expect(
      featureSourceHref("atlas", "doc/product/atlas/index.md", { lines: null, anchor: null }),
    ).toBe("/f/atlas/source/product/atlas/index.md");
  });
});

describe("sourceHref", () => {
  it("keeps a feature document inside its feature", () => {
    expect(sourceHref("doc/product/atlas/section_01.md", { anchor: "a" })).toBe(
      "/f/atlas/source/product/atlas/section_01.md#a",
    );
  });

  it("keeps a standalone plan in file mode", () => {
    expect(sourceHref("doc/plans/PLAN-x.md")).toBe("/files/plans/PLAN-x.md");
  });

  it("keeps lines and anchor on a file mode address", () => {
    expect(sourceHref("doc/notes.md", { lines: { start: 1, end: 2 }, anchor: "x" })).toBe(
      "/files/notes.md?lines=1-2#x",
    );
  });
});
