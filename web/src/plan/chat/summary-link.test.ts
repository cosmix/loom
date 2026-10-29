import { expect, it } from "vitest";

import { summaryLink } from "./turn-footer-rebuilt";

it("maps plan summaries to the engineering pages and keeps product ids", () => {
  expect(summaryLink("plans/draft")).toBe("/engineering/draft");
  expect(summaryLink("product/section_02")).toBe("/product/section_02");
  expect(summaryLink("product")).toBe("/product");
});

it("a leading slash or dot segments cannot form a protocol-relative or parent link", () => {
  expect(summaryLink("//evil.example/page")).toBe("/evil.example/page");
  expect(summaryLink("/product/section_02")).toBe("/product/section_02");
  expect(summaryLink("/../x/./y")).toBe("/x/y");
});

it("appends the anchor as an encoded fragment", () => {
  expect(summaryLink("product/section_02", "s-3fa9 c2")).toBe("/product/section_02#s-3fa9%20c2");
  expect(summaryLink("product/section_02", null)).toBe("/product/section_02");
});
