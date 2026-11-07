import { afterEach, describe, expect, it, vi } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

import { handleCoverageKey, moveTarget } from "./coverage-keys";
import { buildCoverage, lineage, visibleKeys, type CoverageModel } from "./coverage-model";

const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";
const PLAN = "doc/plans/demo/PLAN-first.md";

function demo(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function model(view: TraceView = demo()): CoverageModel {
  return buildCoverage(view, "demo");
}

function titlesOf(coverage: CoverageModel, column: string): string[] {
  return coverage.cards.filter((card) => card.column === column).map((card) => card.title);
}

/** The fixture with a heading nothing covers. */
function withGap(): TraceView {
  const view = demo();
  view.headings.push({ path: S01, anchor: "five", title: "Five", covered_by: [], findings: [] });
  return view;
}

describe("buildCoverage", () => {
  it("draws the five columns from the trace", () => {
    const coverage = model();
    expect(titlesOf(coverage, "vision")).toEqual(["Alpha", "Beta", "Gamma"]);
    expect(titlesOf(coverage, "spec")).toEqual(["One", "Two", "Three", "Four"]);
    expect(titlesOf(coverage, "metaplan")).toHaveLength(2);
    // Plan second has no file yet, so only plan first and its stage are cards.
    expect(titlesOf(coverage, "plans")).toEqual(["First", "first · build"]);
    expect(titlesOf(coverage, "findings")).toEqual([
      "F-1 Rebuild ignores briefs",
      "F-2 Rename the flag",
    ]);
  });

  it("groups headings under their section's title", () => {
    const groups = model().cards.filter((card) => card.column === "spec");
    expect(groups.map((card) => card.group)).toEqual(["Alpha", "Alpha", "Beta", "Beta"]);
  });

  it("flags only what needs a reader's eye", () => {
    const coverage = model(withGap());
    const flagged = coverage.cards
      .filter((card) => card.description !== null)
      .map((card) => [card.title, card.state, card.description]);
    expect(flagged).toEqual([
      ["Beta", "revised", "Revised since settled"],
      ["Five", "uncovered", "Not covered"],
      ["F-1 Rebuild ignores briefs", "open", "Open finding"],
    ]);
  });

  it("flags no done work, even when it moved since it was settled", () => {
    const view = demo();
    const plan = view.documents.find((doc) => doc.path === PLAN)!;
    Object.assign(plan, { settle: "upstream_changed", settleable: false });
    const card = model(view).byKey.get(`plan:${PLAN}`)!;
    expect([card.state, card.description]).toEqual(["plain", null]);
  });

  it("opens each node where it lives", () => {
    const hrefs = Object.fromEntries(model().cards.map((card) => [card.key, card.href]));
    expect(hrefs[`vision:${S01}`]).toBe("/f/demo/product/section_01");
    expect(hrefs[`spec:${S01}#two`]).toBe("/f/demo/source/product/demo/section_01.md#two");
    expect(hrefs[`plan:${PLAN}`]).toBe("/f/demo/engineering/first");
    expect(hrefs[`stage:${PLAN}#build`]).toBe(
      "/f/demo/source/plans/demo/PLAN-first.md#stage%3Abuild",
    );
    expect(hrefs["finding:F-1"]).toBe("/f/demo/findings#finding-F-1");
  });
});

describe("lineage", () => {
  it("runs from the vision row through the heading to its plan, stage and findings", () => {
    const coverage = model();
    expect(lineage(coverage, `spec:${S01}#two`)).toEqual({
      up: [`vision:${S01}`],
      down: [`plan:${PLAN}`, `stage:${PLAN}#build`, "finding:F-1"],
    });
    const card = coverage.byKey.get(`spec:${S01}#two`)!;
    expect(card.lineage).toEqual([
      `vision:${S01}`,
      `plan:${PLAN}`,
      `stage:${PLAN}#build`,
      "finding:F-1",
    ]);
  });

  it("reaches a plan from the entry that names it and the headings it covers", () => {
    const coverage = model();
    expect(lineage(coverage, `plan:${PLAN}`).up).toEqual([
      `vision:${S01}`,
      `spec:${S01}#one`,
      `spec:${S01}#two`,
      "entry:1-first-plan-firstmd",
    ]);
  });

  it("hangs a finding off the plan it names", () => {
    const view = demo();
    view.findings[1]!.about = [{ path: PLAN, anchor: null }];
    expect(lineage(model(view), "finding:F-2").up).toContain(`plan:${PLAN}`);
  });
});

describe("visibleKeys", () => {
  it("shows every card for All", () => {
    const coverage = model();
    expect(visibleKeys(coverage, "all").size).toBe(coverage.cards.length);
  });

  it("shows the uncovered headings with the section they sit in", () => {
    expect([...visibleKeys(model(withGap()), "uncovered")].sort()).toEqual(
      [`spec:${S01}#five`, `vision:${S01}`].sort(),
    );
  });

  it("shows the documents that moved since settling and what hangs off them", () => {
    const shown = visibleKeys(model(), "changed");
    expect(shown.has(`vision:${S02}`)).toBe(true);
    expect(shown.has(`spec:${S02}#three`)).toBe(true);
    expect(shown.has("finding:F-2")).toBe(true);
    expect(shown.has(`vision:${S01}`)).toBe(false);
  });

  it("shows the open findings with their heading and section", () => {
    expect([...visibleKeys(model(), "open")].sort()).toEqual(
      ["finding:F-1", `spec:${S01}#two`, `vision:${S01}`].sort(),
    );
  });
});

describe("moveTarget", () => {
  const coverage = model();
  const all = visibleKeys(coverage, "all");

  it("walks a column with j and k and stops at its ends", () => {
    expect(moveTarget(coverage, all, `spec:${S01}#one`, "j")).toBe(`spec:${S01}#two`);
    expect(moveTarget(coverage, all, `spec:${S01}#two`, "k")).toBe(`spec:${S01}#one`);
    expect(moveTarget(coverage, all, `spec:${S01}#one`, "k")).toBeNull();
    expect(moveTarget(coverage, all, `spec:${S02}#four`, "j")).toBeNull();
  });

  it("goes down with l and up with h, to the first card of the lineage", () => {
    expect(moveTarget(coverage, all, `vision:${S01}`, "l")).toBe(`spec:${S01}#one`);
    expect(moveTarget(coverage, all, `spec:${S01}#one`, "l")).toBe("entry:1-first-plan-firstmd");
    expect(moveTarget(coverage, all, `plan:${PLAN}`, "h")).toBe(`spec:${S01}#one`);
    expect(moveTarget(coverage, all, `spec:${S01}#one`, "h")).toBe(`vision:${S01}`);
    expect(moveTarget(coverage, all, `vision:${S01}`, "h")).toBeNull();
  });

  it("skips a card the filter hides", () => {
    const shown = new Set(all);
    shown.delete(`spec:${S01}#two`);
    expect(moveTarget(coverage, shown, `spec:${S01}#one`, "j")).toBe(`spec:${S02}#three`);
  });

  it("ignores a card it does not know and a key it does not own", () => {
    expect(moveTarget(coverage, all, "nope", "j")).toBeNull();
    expect(moveTarget(coverage, all, `vision:${S01}`, "x")).toBeNull();
  });
});

describe("handleCoverageKey", () => {
  const coverage = model();
  const visible = visibleKeys(coverage, "all");

  afterEach(() => {
    document.body.replaceChildren();
  });

  function press(key: string, target: Element, patch: Partial<KeyboardEvent> = {}) {
    const event = {
      key,
      target,
      altKey: false,
      ctrlKey: false,
      metaKey: false,
      defaultPrevented: false,
      preventDefault: vi.fn(),
      ...patch,
    };
    const focus = vi.fn();
    const open = vi.fn();
    handleCoverageKey(event as never, { model: coverage, visible, focus, open });
    return { event, focus, open };
  }

  function card(key: string): HTMLElement {
    const button = document.createElement("button");
    button.dataset.key = key;
    document.body.append(button);
    return button;
  }

  it("moves focus on l and opens the node on Enter", () => {
    const here = card(`vision:${S01}`);
    expect(press("l", here).focus).toHaveBeenCalledWith(`spec:${S01}#one`);
    const entered = press("Enter", here);
    expect(entered.open).toHaveBeenCalledWith("/f/demo/product/section_01");
    expect(entered.event.preventDefault).toHaveBeenCalled();
  });

  it("stays out of the way while typing, with a modifier or with a dialog open", () => {
    const here = card(`vision:${S01}`);
    const field = document.createElement("input");
    document.body.append(field);
    expect(press("l", field).focus).not.toHaveBeenCalled();
    expect(press("l", here, { ctrlKey: true }).focus).not.toHaveBeenCalled();
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "alertdialog");
    document.body.append(dialog);
    expect(press("l", here).focus).not.toHaveBeenCalled();
    expect(press("Enter", here).open).not.toHaveBeenCalled();
  });

  it("ignores keys it does not own and presses outside a card", () => {
    const here = card(`vision:${S01}`);
    expect(press("x", here).event.preventDefault).not.toHaveBeenCalled();
    expect(press("l", document.body).focus).not.toHaveBeenCalled();
  });
});
