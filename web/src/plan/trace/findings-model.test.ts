import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import type { SessionView } from "@/plan/api/schema-chat";
import {
  traceViewSchema,
  type FindingCheckKind,
  type TraceFinding,
  type TraceView,
} from "@/plan/api/schema-trace";

import { dismissReason, ledgerPath } from "./findings-actions";
import {
  addressOffer,
  addressRunLive,
  cardState,
  groupFindings,
  groupTitle,
  isFullCard,
  nodeLabel,
  openCount,
  runOf,
  shortRun,
  stateText,
  tally,
} from "./findings-model";

function demo(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function entryOf(
  id: string,
  source: string | null,
  kind: "open" | "addressed" | "dismissed" = "open",
  check: FindingCheckKind = "verified",
): TraceFinding {
  const [first] = demo().findings;
  return {
    finding: {
      ...first!.finding,
      id,
      title: `Title ${id}`,
      source,
      severity: "medium",
      status: { kind, detail: kind === "open" ? null : "no" },
    },
    about: [],
    check: { kind: check, duplicate_of: check === "duplicate" ? "F-1" : null },
    changed: [],
  };
}

function session(offer: string | null, feature: string | null, state = "idle"): SessionView {
  return { offer, feature, state } as SessionView;
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
  it("splits open findings by their check, counts the rest by status and leaves out zeros", () => {
    const all = [
      entryOf("F-1", null),
      entryOf("F-2", null),
      entryOf("F-3", null, "open", "unverified"),
      entryOf("F-4", null, "open", "stale"),
      entryOf("F-5", null, "open", "duplicate"),
      entryOf("F-6", null, "addressed"),
      entryOf("F-7", null, "dismissed"),
    ];
    expect(tally(all)).toBe("2 to address · 2 to verify · 1 addressed · 1 dismissed");
    expect(tally([])).toBe("");
  });
});

describe("openCount and the address offer", () => {
  it("counts only open, verified findings", () => {
    const view = demo();
    // The fixture: F-1 open and verified, F-2 dismissed.
    expect(view.findings).toHaveLength(2);
    expect(openCount(view)).toBe(1);
    view.findings.push(
      entryOf("F-5", null),
      entryOf("F-6", null, "open", "duplicate"),
      entryOf("F-7", null, "open", "stale"),
    );
    expect(openCount(view)).toBe(2);
  });

  it("offers Address the findings only while one is actionable", () => {
    const view = demo();
    expect(addressOffer(view, "demo")).toEqual({
      id: "address-findings",
      kind: "address_findings",
      label: "Address the findings",
      target: null,
      feature: "demo",
    });
    view.findings[0]!.check = { kind: "unverified", duplicate_of: null };
    expect(addressOffer(view, "demo")).toBeUndefined();
  });

  it("reads a live address run from the open session's offer and feature", () => {
    expect(addressRunLive(session("address-findings", "demo"), "demo")).toBe(true);
    expect(addressRunLive(session("address-findings", "other"), "demo")).toBe(false);
    expect(addressRunLive(session("review-feature", "demo"), "demo")).toBe(false);
    expect(addressRunLive(session("address-findings", "demo", "ended"), "demo")).toBe(false);
    expect(addressRunLive(null, "demo")).toBe(false);
  });
});

describe("card states and nodeLabel", () => {
  it("reads the check of an open finding and the status of any other", () => {
    const state = (entry: TraceFinding, addressing = false) => cardState(entry, addressing);
    expect(state(entryOf("F-1", null))).toBe("open");
    expect(state(entryOf("F-1", null), true)).toBe("addressing");
    expect(state(entryOf("F-1", null, "open", "stale"), true)).toBe("stale");
    expect(state(entryOf("F-1", null, "open", "duplicate"))).toBe("duplicate");
    expect(state(entryOf("F-1", null, "addressed"), true)).toBe("addressed");
    expect(isFullCard("unverified")).toBe(true);
    expect(isFullCard("duplicate")).toBe(false);
    expect(isFullCard("addressed")).toBe(false);
  });

  it("puts each state in words", () => {
    const open = entryOf("F-1", null);
    expect(stateText(open, "open")).toBe("Open · medium");
    expect(stateText(open, "addressing")).toBe("Being addressed");
    const duplicate = entryOf("F-2", null, "open", "duplicate");
    expect(stateText(duplicate, "duplicate")).toBe("Duplicate of F-1");
    const dismissed = entryOf("F-3", null, "dismissed");
    expect(stateText(dismissed, "dismissed")).toBe("Dismissed: no");
    const left = entryOf("F-4", null);
    left.finding.status = { kind: "unaddressed", detail: "the plan is done" };
    expect(stateText(left, "unaddressed")).toBe("Could not address: the plan is done");
  });

  it("names a heading, an entry and a document", () => {
    const view = demo();
    const section = "doc/product/demo/section_01.md";
    expect(nodeLabel(view, { path: section, anchor: "two" }, "demo")).toBe("Two");
    expect(nodeLabel(view, { path: section, anchor: null }, "demo")).toBe("Alpha");
    expect(
      nodeLabel(
        view,
        { path: "doc/plans/demo/index.md", anchor: "2-second-plan-secondmd" },
        "demo",
      ),
    ).toBe("2. Second (`PLAN-second.md`)");
    expect(nodeLabel(view, { path: "doc/plans/demo/other.md", anchor: null }, "demo")).toBe(
      "other.md",
    );
  });
});

describe("plan stage labels", () => {
  it("reads a plan stage as the short slug and the stage, and a whole plan by its title", () => {
    const view = demo();
    const plan = view.documents.find((doc) => doc.path === "doc/plans/demo/PLAN-first.md")!;
    plan.slug = "demo-first";
    const node = { path: plan.path, anchor: "stage:build" };
    expect(nodeLabel(view, node, "demo")).toBe("first › build");
    expect(nodeLabel(view, { path: plan.path, anchor: null }, "demo")).toBe(plan.title);
  });

  it("falls back to the file's stem when the plan has no slug yet", () => {
    const view = demo();
    const plan = view.documents.find((doc) => doc.path === "doc/plans/demo/PLAN-first.md")!;
    plan.slug = null;
    expect(nodeLabel(view, { path: plan.path, anchor: "stage:build" }, "demo")).toBe(
      "first › build",
    );
  });
});

describe("finding actions", () => {
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
