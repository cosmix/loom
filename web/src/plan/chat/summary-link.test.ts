import { expect, it } from "vitest";

import { summaryLink } from "./turn-footer-rebuilt";

const planFeature = (slug: string) => (slug === "x" ? "demo" : null);

it.each([
  ["demo/product/section_01", "/f/demo/product/section_01"],
  ["plans/x", "/f/demo/engineering/x"],
  ["plans/y", "/plans/y"],
  ["overview/demo/product", "/f/demo/product"],
  ["overview/demo/engineering", "/f/demo/engineering"],
])("maps server summary %s to %s", (id, href) => {
  expect(summaryLink(id, planFeature)).toBe(href);
});

it.each([
  "//evil.example/page",
  "/demo/product/section_01",
  "/../x/./y",
  "product/section_01",
  "product",
  "demo/engineering/x",
  "plans/../x",
  "plans/",
])("rejects unroutable summary %s", (id) => {
  expect(summaryLink(id, planFeature)).toBeNull();
});

it("appends the anchor as an encoded fragment", () => {
  expect(summaryLink("demo/product/section_01", planFeature, "s-3fa9 c2")).toBe(
    "/f/demo/product/section_01#s-3fa9%20c2",
  );
  expect(summaryLink("plans/x", planFeature, null)).toBe("/f/demo/engineering/x");
  expect(summaryLink("product", planFeature, "sentence")).toBeNull();
});
