import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type CoverRef, type TraceView } from "@/plan/api/schema-trace";

import {
  buildCoverage,
  filterRows,
  findAnchor,
  rowElementId,
  STATE_WORDS,
  type CoverageRow,
} from "./coverage-model";

const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const S01 = "doc/product/demo/section_01.md";

function demo(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function patchPlan(view: TraceView, patch: Partial<TraceView["documents"][number]>): void {
  Object.assign(
    view.documents.find((doc) => doc.path === FIRST_PLAN)!,
    patch,
  );
}

function rowsOf(view: TraceView, feature = "demo"): Map<string, CoverageRow> {
  const rows = buildCoverage(view, feature).sections.flatMap((section) => section.rows);
  return new Map(rows.map((row) => [row.anchor, row]));
}

describe("buildCoverage sections", () => {
  it("orders sections by number and counts the covered headings of each", () => {
    const { sections } = buildCoverage(demo(), "demo");
    expect(sections.map((s) => [s.id, s.number, s.covered, s.total])).toEqual([
      ["section_01", "01", 2, 2],
      ["section_02", "02", 2, 2],
      ["section_03", "03", 0, 0],
    ]);
    expect(sections[0]!.settle).toBe("settled");
    expect(sections[1]!.settle).toBe("revised");
  });

  it("keeps a section that is not written as a section with no rows", () => {
    const gamma = buildCoverage(demo(), "demo").sections[2]!;
    expect(gamma).toMatchObject({ title: "Gamma", written: false, rows: [], settle: null });
  });
});

describe("buildCoverage rows", () => {
  it("joins an entry and the plan it leads to into one chain, with the plan's state in words", () => {
    const one = rowsOf(demo()).get("one")!;
    expect(one.chains).toHaveLength(1);
    expect(one.chains[0]).toMatchObject({
      entry: "1. First (`PLAN-first.md`)",
      plan: "first",
      planState: "Written",
      stage: null,
      planHref: "/f/demo/engineering/first",
    });
    expect(one.state).toBe("planned");
  });

  it("names the stage that covers a heading, and drops the plan-wide cover beside it", () => {
    const view = demo();
    const two = view.headings.find((heading) => heading.anchor === "two")!;
    const wholePlan: CoverRef = {
      kind: "plan",
      path: FIRST_PLAN,
      anchor: null,
      slug: "first",
      stage: null,
    };
    two.covered_by.push(wholePlan);
    const chains = rowsOf(view).get("two")!.chains;
    expect(chains.map((chain) => [chain.plan, chain.stage])).toEqual([["first", "build"]]);
    expect(chains[0]!.stageHref).toBe("/f/demo/source/plans/demo/PLAN-first.md#stage%3Abuild");
  });

  it("reads a covering entry whose plan is not written as planned, with its slug", () => {
    const three = rowsOf(demo()).get("three")!;
    expect(three.chains[0]).toMatchObject({
      plan: "second",
      planPath: null,
      planHref: null,
      planState: "Not written",
    });
    expect(three.state).toBe("planned");
  });

  it("drops the feature prefix from a plan's slug", () => {
    const view = demo();
    patchPlan(view, { slug: "demo-first" });
    expect(rowsOf(view).get("one")!.chains[0]!.plan).toBe("first");
  });

  it("reads built when every covering plan is done", () => {
    const view = demo();
    patchPlan(view, { progress: "done", settleable: false });
    expect(rowsOf(view).get("one")!.state).toBe("built");
    // Heading three is covered by an entry with no plan, which is not done.
    expect(rowsOf(view).get("three")!.state).toBe("planned");
  });

  it("reads changed when a covering plan's spec moved since the plan was settled", () => {
    const view = demo();
    patchPlan(view, { settle: "upstream_changed" });
    const rows = rowsOf(view);
    expect(rows.get("one")!.state).toBe("changed");
    expect(rows.get("one")!.chains[0]!.changed).toBe(true);
    expect(rows.get("three")!.state).toBe("planned");
  });

  it("reads uncovered when nothing covers the heading", () => {
    const view = demo();
    view.headings.push({ path: S01, anchor: "five", title: "Five", covered_by: [], findings: [] });
    const five = rowsOf(view).get("five")!;
    expect(five.state).toBe("uncovered");
    expect(five.chains).toEqual([]);
    expect(Object.values(STATE_WORDS)).toContain("Not covered");
  });

  it("lists only the open findings about a heading", () => {
    const rows = rowsOf(demo());
    expect(rows.get("two")!.open).toEqual([{ id: "F-1", title: "Rebuild ignores briefs" }]);
    // F-2 is dismissed.
    expect(rows.get("three")!.open).toEqual([]);
  });
});

describe("tiles and filters", () => {
  function withGap(): TraceView {
    const view = demo();
    view.headings.push({ path: S01, anchor: "five", title: "Five", covered_by: [], findings: [] });
    patchPlan(view, { settle: "upstream_changed" });
    return view;
  }

  it("counts all headings, the uncovered, the changed and those with open findings", () => {
    expect(buildCoverage(withGap(), "demo").tiles).toEqual({
      all: 5,
      uncovered: 1,
      changed: 2,
      open: 1,
    });
  });

  it("shows only the rows a filter picks, in the sections that still hold one", () => {
    const model = buildCoverage(withGap(), "demo");
    const titles = (filter: Parameters<typeof filterRows>[1]) =>
      filterRows(model, filter).map((s) => [s.id, s.rows.map((row) => row.title)]);
    expect(titles("uncovered")).toEqual([["section_01", ["Five"]]]);
    expect(titles("changed")).toEqual([["section_01", ["One", "Two"]]]);
    expect(titles("open")).toEqual([["section_01", ["Two"]]]);
    // `all` keeps every section, the one that is not written too.
    expect(filterRows(model, "all").map((s) => s.id)).toEqual([
      "section_01",
      "section_02",
      "section_03",
    ]);
  });

  it("keeps a section's own counts whatever the filter shows", () => {
    const [section] = filterRows(buildCoverage(withGap(), "demo"), "uncovered");
    expect([section!.covered, section!.total]).toEqual([2, 3]);
  });

  it("counts nothing for a trace with no sections", () => {
    const model = buildCoverage(
      traceViewSchema.parse({ ...demo(), documents: [], headings: [] }),
      "demo",
    );
    expect(model.tiles).toEqual({ all: 0, uncovered: 0, changed: 0, open: 0 });
    expect(model.sections).toEqual([]);
  });
});

describe("findAnchor", () => {
  it("finds a section by its file name and a heading by its anchor, opening that row", () => {
    const model = buildCoverage(demo(), "demo");
    expect(findAnchor(model, "section_02")).toEqual({ elementId: "section_02", rowKey: null });
    const row = model.sections[0]!.rows[1]!;
    expect(findAnchor(model, "two")).toEqual({
      elementId: rowElementId(model.sections[0]!, row),
      rowKey: `${S01}#two`,
    });
    const named = { elementId: rowElementId(model.sections[0]!, row), rowKey: `${S01}#two` };
    expect(findAnchor(model, "section_01:two")).toEqual(named);
    expect(findAnchor(model, "section_02:two")).toBeNull();
    expect(findAnchor(model, "nope")).toBeNull();
    expect(findAnchor(model, "section_03")).toEqual({ elementId: "section_03", rowKey: null });
  });
});
