import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceFinding, type TraceView } from "@/plan/api/schema-trace";

import { dismissReason, foldSeed, ledgerPath, planSeed } from "./findings-actions";
import {
  foldableEntries,
  foldChoices,
  groupFindings,
  groupTitle,
  nodeLabel,
  openCount,
  runOf,
  shortRun,
  statusText,
  tally,
} from "./findings-model";

function demo(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function entryOf(
  id: string,
  source: string | null,
  kind: "open" | "dismissed" = "open",
): TraceFinding {
  const [first] = demo().findings;
  return {
    finding: {
      ...first!.finding,
      id,
      title: `Title ${id}`,
      source,
      status: { kind, detail: kind === "open" ? null : "no" },
    },
    about: [],
  };
}

describe("runOf", () => {
  it.each([
    ["REVIEW-PLAN-first.md#open-findings (build, 1a2b3c4d5e6f)", "REVIEW-PLAN-first"],
    ["REVIEW-DONE-PLAN-first.md#open-questions (build, 1a2b3c4d5e6f)", "REVIEW-DONE-PLAN-first"],
    ["review 2026-11-06", null],
    [null, null],
  ])("reads %s as the run %s", (source, run) => {
    expect(runOf(source)).toBe(run);
  });
});

describe("groupFindings", () => {
  it("puts the run whose first finding is latest first and the reviews last", () => {
    const groups = groupFindings([
      entryOf("F-1", "REVIEW-PLAN-a.md#open-findings (s, 1)"),
      entryOf("F-2", null),
      entryOf("F-3", "REVIEW-PLAN-b.md#open-findings (s, 2)"),
      entryOf("F-4", "REVIEW-PLAN-a.md#open-findings (s, 3)"),
      entryOf("F-5", "review 2026-11-06"),
    ]);
    // Run a's first finding precedes run b's, so b is the newer run although a has the last finding.
    expect(groups.map((group) => group.label)).toEqual([
      "REVIEW-PLAN-b",
      "REVIEW-PLAN-a",
      "Reviews",
    ]);
    expect(groups[1]!.findings.map(({ finding }) => finding.id)).toEqual(["F-1", "F-4"]);
    expect(groups[2]).toMatchObject({ run: null });
    expect(groups[2]!.findings.map(({ finding }) => finding.id)).toEqual(["F-2", "F-5"]);
  });

  it("makes no group for no findings", () => {
    expect(groupFindings([])).toEqual([]);
  });
});

describe("group titles", () => {
  it.each([
    ["REVIEW-PLAN-demo-03-summaries", "03-summaries"],
    ["REVIEW-PLAN-first", "first"],
    ["REVIEW-DONE-PLAN-first", "first"],
    ["REVIEW-demo-x", "x"],
  ])("shortens %s to %s", (run, short) => {
    expect(shortRun(run, "demo")).toBe(short);
  });

  it("titles a run by its short slug and the rest Reviews", () => {
    const [run, reviews] = groupFindings([
      entryOf("F-1", "REVIEW-PLAN-demo-03-summaries.md#open-findings (s, 1)"),
      entryOf("F-2", null),
    ]);
    expect(groupTitle(run!, "demo")).toBe("Run of 03-summaries");
    expect(groupTitle(reviews!, "demo")).toBe("Reviews");
  });
});

describe("tally", () => {
  it("counts each status in words and leaves out the zeros", () => {
    const planned = entryOf("F-3", null);
    planned.finding.status = { kind: "planned", detail: "PLAN-x.md" };
    const all = [
      entryOf("F-1", null),
      entryOf("F-2", null),
      planned,
      entryOf("F-4", null, "dismissed"),
    ];
    expect(tally(all)).toBe("2 open · 1 planned · 1 dismissed");
    expect(tally([])).toBe("");
  });
});

describe("openCount", () => {
  it("counts the findings still open, not the total", () => {
    const view = demo();
    expect(view.findings).toHaveLength(2);
    expect(openCount(view)).toBe(1);
    view.findings.push(entryOf("F-3", null), entryOf("F-4", null, "dismissed"));
    expect(openCount(view)).toBe(2);
  });
});

describe("foldableEntries", () => {
  it("offers entries with no plan or a plan that has not started", () => {
    const view = demo();
    const [first, second] = view.entries;
    const withPlan = (name: string) => ({ ...first!, plan: `doc/plans/demo/${name}` });
    view.entries = [
      first!,
      second!,
      withPlan("IN_PROGRESS-PLAN-a.md"),
      withPlan("DONE-PLAN-b.md"),
      withPlan("RETIRED-PLAN-c.md"),
    ];
    expect(foldableEntries(view)).toEqual([first, second]);
  });
});

describe("foldChoices", () => {
  const section = "doc/product/demo/section_01.md";

  it("puts the entry covering most of the finding's About nodes first", () => {
    const view = demo();
    const [first, second] = view.entries;
    // Entry two covers all of section 02, entry one only heading one of section 01.
    const about = [
      { path: "doc/product/demo/section_02.md", anchor: "three" },
      { path: "doc/product/demo/section_02.md", anchor: null },
    ];
    const choices = foldChoices(view, about);
    expect(choices.suggested).toBe(second);
    expect(choices.entries).toEqual([second, first]);
    expect(choices.leftOut).toEqual([]);
  });

  it("suggests nothing when no entry covers what the finding is about", () => {
    const view = demo();
    const choices = foldChoices(view, [{ path: section, anchor: "two" }]);
    expect(choices.suggested).toBeNull();
    expect(choices.entries).toEqual(view.entries);
  });

  it("names the entries left out because their plan has started", () => {
    const view = demo();
    const [first, second] = view.entries;
    view.entries = [{ ...first!, plan: "doc/plans/demo/IN_PROGRESS-PLAN-first.md" }, second!];
    const choices = foldChoices(view, [{ path: section, anchor: "one" }]);
    expect(choices.leftOut.map((entry) => entry.anchor)).toEqual([first!.anchor]);
    expect(choices.entries).toEqual([second]);
    expect(choices.suggested).toBeNull();
  });
});

describe("statusText and nodeLabel", () => {
  it("reads every status kind", () => {
    expect(statusText({ kind: "open", detail: null })).toBe("Open");
    expect(statusText({ kind: "planned", detail: "PLAN-x.md" })).toBe("Planned in PLAN-x.md");
    expect(statusText({ kind: "folded", detail: "metaplan#x" })).toBe("Folded into metaplan#x");
    expect(statusText({ kind: "dismissed", detail: "dup" })).toBe("Dismissed: dup");
  });

  it("names a heading, an entry and a document", () => {
    const view = demo();
    const section = "doc/product/demo/section_01.md";
    expect(nodeLabel(view, { path: section, anchor: "two" })).toBe("Two");
    expect(nodeLabel(view, { path: section, anchor: null })).toBe("Alpha");
    expect(
      nodeLabel(view, { path: "doc/plans/demo/index.md", anchor: "2-second-plan-secondmd" }),
    ).toBe("2. Second (`PLAN-second.md`)");
    expect(nodeLabel(view, { path: "doc/plans/demo/other.md", anchor: null })).toBe("other.md");
  });
});

describe("finding actions", () => {
  const view = demo();
  const { finding } = view.findings[0]!;

  it("seeds Plan follow-ups with the finding, its About tokens and its text", () => {
    expect(planSeed(finding)).toBe(
      "Plan finding F-1: Rebuild ignores briefs\n" +
        "About: PLAN-first.md#stage:build, section_01#two\n\n" +
        "The rebuild budget counts plans but not briefs.",
    );
  });

  it("seeds a fold with the entry's title and anchor", () => {
    expect(foldSeed(finding, view.entries[1]!)).toBe(
      "Fold finding F-1 into metaplan entry 2. Second (`PLAN-second.md`) " +
        "(metaplan#2-second-plan-secondmd): Rebuild ignores briefs\n" +
        "About: PLAN-first.md#stage:build, section_01#two",
    );
  });

  it("trims a dismissal reason and refuses an empty one", () => {
    expect(dismissReason("  duplicate \n")).toBe("duplicate");
    expect(dismissReason(" \t ")).toBeNull();
    expect(dismissReason("two\nlines\u0007 here")).toBe("two lines here");
    expect(dismissReason("\u0000\n")).toBeNull();
    expect(dismissReason("x".repeat(250))).toBe("x".repeat(200));
  });

  it("keeps the ledger under the feature's plans folder", () => {
    expect(ledgerPath("demo")).toBe("doc/plans/demo/findings.md");
  });
});
