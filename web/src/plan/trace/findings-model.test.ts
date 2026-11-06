import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceFinding, type TraceView } from "@/plan/api/schema-trace";

import { dismissReason, foldSeed, ledgerPath, planSeed } from "./findings-actions";
import {
  foldableEntries,
  groupFindings,
  nodeLabel,
  openCount,
  runOf,
  statusText,
} from "./findings-model";

function demo(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function entryOf(id: string, source: string | null, kind: "open" | "dismissed" = "open") {
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
  } satisfies TraceFinding;
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
  it("puts the run whose newest finding is latest first and the reviews last", () => {
    const groups = groupFindings([
      entryOf("F-1", "REVIEW-PLAN-a.md#open-findings (s, 1)"),
      entryOf("F-2", null),
      entryOf("F-3", "REVIEW-PLAN-b.md#open-findings (s, 2)"),
      entryOf("F-4", "REVIEW-PLAN-a.md#open-findings (s, 3)"),
      entryOf("F-5", "review 2026-11-06"),
    ]);
    expect(groups.map((group) => group.label)).toEqual([
      "REVIEW-PLAN-a",
      "REVIEW-PLAN-b",
      "Reviews",
    ]);
    expect(groups[0]!.findings.map(({ finding }) => finding.id)).toEqual(["F-1", "F-4"]);
    expect(groups[2]).toMatchObject({ run: null });
    expect(groups[2]!.findings.map(({ finding }) => finding.id)).toEqual(["F-2", "F-5"]);
  });

  it("makes no group for no findings", () => {
    expect(groupFindings([])).toEqual([]);
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
  });

  it("keeps the ledger under the feature's plans folder", () => {
    expect(ledgerPath("demo")).toBe("doc/plans/demo/findings.md");
  });
});
