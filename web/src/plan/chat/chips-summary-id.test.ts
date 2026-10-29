import { expect, it } from "vitest";

import { summaryIdOf } from "./chips";

it("summaryIdOf strips the doc prefix, the summaries directory and the extension", () => {
  expect(summaryIdOf("doc/loom/summaries/product.md")).toBe("product");
  expect(summaryIdOf("loom/summaries/section_02.md")).toBe("section_02");
  expect(summaryIdOf("doc/product/section_02.md")).toBe("product/section_02");
});
