import { expect, it } from "vitest";

import { summaryIdOf } from "./chips";

it("summaryIdOf strips the doc prefix, the summaries directory and the extension", () => {
  expect(summaryIdOf("doc/loom/summaries/demo/product/section_01.md")).toBe(
    "demo/product/section_01",
  );
  expect(summaryIdOf("loom/summaries/plans/x.md")).toBe("plans/x");
  expect(summaryIdOf("doc/loom/summaries/plans/y.md")).toBe("plans/y");
});
