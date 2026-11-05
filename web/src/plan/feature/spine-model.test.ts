import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";

import { groupOfPathname, summaryPathnameOfSource } from "./spine-model";

it.each([
  ["/f/atlas/vision", "vision"],
  ["/f/atlas/product", "product"],
  ["/f/atlas/product/section_01", "product"],
  ["/f/atlas/engineering/atlas-01", "plans"],
  ["/f/atlas/metaplan", "metaplan"],
  ["/f/atlas/run", "run"],
  ["/f/atlas/built", "run"],
])("selects the group of %s", (pathname, group) => {
  expect(groupOfPathname(pathname)).toBe(group);
});

it("selects no group for the removed Pressure page", () => {
  expect(groupOfPathname("/f/atlas/pressure")).toBeNull();
});

it.each([
  ["/f/atlas/source/product/atlas/index.md", "vision"],
  ["/f/atlas/source/product/atlas/section_02.md", "product"],
  ["/f/atlas/source/plans/atlas/index.md", "metaplan"],
  ["/f/atlas/source/plans/atlas/METAPLAN.md", "metaplan"],
  ["/f/atlas/source/plans/atlas/PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/IN_PROGRESS-PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/DONE-PLAN-x.md", "plans"],
])("selects the group of the document shown at %s", (pathname, group) => {
  expect(groupOfPathname(pathname)).toBe(group);
});

it("selects no group for a source path it cannot read", () => {
  expect(groupOfPathname("/f/atlas/source/product/atlas/%E0%A4%A.md")).toBeNull();
  expect(groupOfPathname("/f/atlas/source")).toBeNull();
  expect(groupOfPathname("/f/atlas/source/knowledge/x.md")).toBeNull();
});

it.each([
  "/f/atlas/source/product/other/section_01.md",
  "/f/atlas/source/product/other/index.md",
  "/f/atlas/source/plans/other/PLAN-x.md",
  "/f/atlas/source/plans/PLAN-x.md",
])("selects no group for the foreign-feature document at %s", (pathname) => {
  expect(groupOfPathname(pathname)).toBeNull();
});

function atlas(patch: Record<string, unknown> = {}): ChecklistView {
  const section = (number: string, written: boolean) => ({
    number,
    name: `Section ${number}`,
    written,
    open_questions: 0,
  });
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    sections: [section("01", true), section("02", false)],
    plan_rows: [
      {
        slug: "x y",
        path: "doc/plans/atlas/IN_PROGRESS-PLAN-x y.md",
        title: "X",
        state: "running",
        pressure_date: null,
        listed: true,
      },
    ],
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Roadmap" }],
    ...patch,
  });
}

it.each([
  ["/f/atlas/source/product/atlas/index.md", "/f/atlas/vision"],
  ["/f/atlas/source/product/atlas/section_01.md", "/f/atlas/product/section_01"],
  ["/f/atlas/source/plans/atlas/IN_PROGRESS-PLAN-x%20y.md", "/f/atlas/engineering/x%20y"],
  ["/f/atlas/source/plans/atlas/index.md", "/f/atlas/metaplan"],
  // not written: the child is listed, not linked
  ["/f/atlas/source/product/atlas/section_02.md", null],
  ["/f/atlas/source/product/atlas/section_09.md", null],
  ["/f/atlas/source/product/atlas/notes.md", null],
  ["/f/atlas/source/plans/atlas/PLAN-other.md", null],
  ["/f/atlas/source/product/atlas/%E0%A4%A.md", null],
  ["/f/atlas/vision", null],
])("maps the source route %s to the summary route %s", (pathname, expected) => {
  expect(summaryPathnameOfSource(atlas(), pathname)).toBe(expected);
});

it.each([
  "/f/atlas/source/product/other/section_01.md",
  "/f/atlas/source/product/other/index.md",
  "/f/atlas/source/plans/other/IN_PROGRESS-PLAN-x%20y.md",
  "/f/atlas/source/plans/other/index.md",
])("maps no summary route for the foreign-feature source route %s", (pathname) => {
  expect(summaryPathnameOfSource(atlas(), pathname)).toBeNull();
});

it("maps a metaplan file the checklist does not list to no summary route", () => {
  expect(summaryPathnameOfSource(atlas(), "/f/atlas/source/plans/atlas/round%202.md")).toBeNull();
});
