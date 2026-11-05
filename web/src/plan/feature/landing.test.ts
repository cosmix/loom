import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { landingPath } from "@/plan/feature/landing";

const section = (number: string, written: boolean) => ({
  number,
  name: `Section ${number}`,
  written,
  open_questions: 0,
});

function feature(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    ...patch,
  });
}

const metaplan = [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }];

it("opens a feature with no vision and no metaplan on the vision", () => {
  expect(landingPath(feature({ has_product: false, metaplans: [] }))).toBe("/f/atlas/vision");
});

it("opens a feature with a vision and a missing section on the product overview", () => {
  const sections = [section("01", true), section("02", false)];
  expect(landingPath(feature({ has_product: true, sections }))).toBe("/f/atlas/product");
});

it("opens a feature with a vision and every listed section written on the product overview", () => {
  const sections = [section("01", true), section("02", true)];
  expect(landingPath(feature({ has_product: true, sections }))).toBe("/f/atlas/product");
});

it("opens a feature whose vision lists no sections yet on the product overview", () => {
  expect(landingPath(feature({ has_product: true, sections: [] }))).toBe("/f/atlas/product");
});

it("opens a feature with a product spec and a metaplan on the product overview", () => {
  expect(landingPath(feature({ has_product: true, metaplans: metaplan }))).toBe("/f/atlas/product");
});

it("opens a feature with a metaplan and no vision on the plans", () => {
  expect(landingPath(feature({ has_product: false, metaplans: metaplan }))).toBe(
    "/f/atlas/engineering",
  );
});

it("opens a feature whose every plan is done on what was built", () => {
  const rows = ["one", "two"].map((slug) => ({
    slug,
    path: `doc/plans/atlas/archive/DONE-PLAN-${slug}.md`,
    title: slug,
    state: "done",
    pressure_date: null,
    listed: true,
  }));
  const done = feature({
    has_product: true,
    metaplans: metaplan,
    plan_rows: rows,
    current: { kind: "ready_all_done" },
  });
  expect(landingPath(done)).toBe("/f/atlas/built");
});

it("encodes a feature name that is not a path segment", () => {
  expect(landingPath(feature({ feature: "a feature", has_product: false, metaplans: [] }))).toBe(
    "/f/a%20feature/vision",
  );
});
