import { describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { traceViewSchema, type DocRole, type SettleState } from "@/plan/api/schema-trace";

import { groupOfPathname, rowGlyph, spineGroups, summaryPathnameOfSource } from "./spine-model";

it.each([
  ["/f/atlas/vision", "vision"],
  ["/f/atlas/product", "product"],
  ["/f/atlas/product/section_01", "product"],
  ["/f/atlas/engineering/atlas-01", "plans"],
  ["/f/atlas/metaplan", "metaplan"],
  ["/f/atlas/coverage", "product"],
  ["/f/atlas/findings", "plans"],
])("selects the group of %s", (pathname, group) => {
  expect(groupOfPathname(pathname)).toBe(group);
});

it("selects no group for the removed Pressure and Run pages", () => {
  expect(groupOfPathname("/f/atlas/pressure")).toBeNull();
  expect(groupOfPathname("/f/atlas/run")).toBeNull();
});

it.each([
  ["/f/atlas/source/product/atlas/index.md", "vision"],
  ["/f/atlas/source/product/atlas/section_02.md", "product"],
  ["/f/atlas/source/plans/atlas/index.md", "metaplan"],
  ["/f/atlas/source/plans/atlas/archive/REVIEW-DONE-PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/archive/REVIEW-PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/IN_PROGRESS-PLAN-x.md", "plans"],
  ["/f/atlas/source/plans/atlas/DONE-PLAN-x.md", "plans"],
])("selects the group of the document shown at %s", (pathname, group) => {
  expect(groupOfPathname(pathname)).toBe(group);
});

it("selects no group for a stray file under the plans folder", () => {
  expect(groupOfPathname("/f/atlas/source/plans/atlas/METAPLAN.md")).toBeNull();
  expect(groupOfPathname("/f/atlas/source/plans/atlas/briefs/common.md")).toBeNull();
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

describe("the row glyph", () => {
  const glyph = (
    planState: string | null,
    settle: SettleState | null,
    written = true,
    role: DocRole = "section",
  ) => rowGlyph({ planState, settle, written, role });

  it("goes by precedence: running, attention, settled, done, not written, draft", () => {
    expect(glyph("running", "settled")).toEqual({ kind: "running", word: "Running" });
    expect(glyph(null, "settled")).toEqual({ kind: "settled", word: "Settled" });
    expect(glyph("done", null)).toEqual({ kind: "done", word: "Done" });
    expect(glyph(null, null, false)).toEqual({ kind: "not-written", word: null });
    expect(glyph(null, "draft")).toEqual({ kind: "draft", word: "Draft" });
  });

  it("turns amber, with the state's own words, for a document that moved since settling", () => {
    expect(glyph(null, "revised")).toEqual({
      kind: "attention",
      word: "Changed since you settled it",
    });
    expect(glyph(null, "stub_changed").word).toBe("Changed since you settled it");
    expect(glyph("written", "upstream_changed", true, "plan")).toEqual({
      kind: "attention",
      word: "Spec changed since you settled it",
    });
    // A moved settle state outranks a plan's own done check.
    expect(glyph("done", "revised").kind).toBe("attention");
  });

  it("names a draft only once the trace says so, and a retired plan before anything else", () => {
    expect(glyph("written", null)).toEqual({ kind: "draft", word: null });
    expect(glyph("retired", "settled")).toEqual({ kind: "retired", word: "Retired" });
  });

  it("gives each spine row exactly one glyph, from the trace", () => {
    const trace = traceViewSchema.parse(structuredClone(traceFixture));
    const rows = spineGroups(atlas(), trace);
    const product = rows.find((group) => group.id === "product")!;
    expect(product.children[0]!.glyph).toBeNull();
    expect(product.children[1]!.glyph?.kind).toBe("draft");
    expect(product.children[2]!.glyph).toEqual({ kind: "not-written", word: null });
    expect(rows.find((group) => group.id === "vision")!.glyph).not.toBeNull();
    expect(product.glyph).toBeNull();
  });
});

describe("a plan row's slug", () => {
  it("drops the feature prefix from a plan's slug and keeps the full slug in the row's title", () => {
    const row = { ...atlas().plan_rows[0]!, slug: "atlas-03", state: "written" };
    const plans = spineGroups(atlas({ plan_rows: [row] })).find((group) => group.id === "plans")!;
    expect(plans.children.at(-1)).toMatchObject({ name: "03", title: "atlas-03" });
  });
});

describe("the trace pages", () => {
  const trace = traceViewSchema.parse(structuredClone(traceFixture));

  it("have no row: the Product spec and Plans groups list only their own documents", () => {
    const rows = spineGroups(atlas(), trace);
    const names = rows.flatMap((group) => group.children.map((child) => child.name));
    expect(names).not.toContain("Coverage");
    expect(names).not.toContain("Findings");
    expect(rows.map((group) => group.id)).toEqual(["vision", "product", "metaplan", "plans"]);
    expect(rows.find((group) => group.id === "plans")!.children.at(-1)!.name).not.toBe("Findings");
  });
});
