import { describe, expect, it } from "vitest";

import type { TreeEntry } from "@/plan/api/schema-files";
import {
  completeText,
  needsAttention,
  questionDocs,
  railNode,
  railSections,
  stageNames,
} from "@/plan/files/rail-model";

function entry(fields: Partial<TreeEntry> = {}): TreeEntry {
  return {
    feature: "feat",
    misplaced: false,
    path: null,
    target: null,
    title: "Entry",
    file_name: null,
    kind: "other",
    state: null,
    locked: false,
    gap: false,
    pressure_date: null,
    errors: 0,
    blurb: null,
    children: [],
    ...fields,
  };
}

const plan = (state: TreeEntry["state"], fields: Partial<TreeEntry> = {}) =>
  entry({ kind: "plan", state, path: `doc/plans/feat/PLAN-${state}.md`, ...fields });

describe("railSections", () => {
  it("makes one section per feature, per stray metaplan, and one for standalone plans", () => {
    const listed = [plan("done"), entry({ kind: "metaplan_entry", gap: true })];
    const metaplan = entry({ kind: "metaplan", path: "doc/plans/feat/index.md", children: listed });
    const stray = entry({
      kind: "metaplan",
      feature: null,
      title: "Stray",
      path: "doc/plans/x.md",
    });
    const alone = plan("written", { feature: null });
    const other = entry({ kind: "other", title: "Notes" });
    const feature = entry({ kind: "feature", title: "feat", children: [metaplan] });

    const { sections, rest } = railSections([feature, stray, alone, other]);

    expect(sections.map((section) => section.name)).toEqual(["feat", "Stray", "Standalone plans"]);
    expect(sections[0]!.metaplan?.path).toBe("doc/plans/feat/index.md");
    expect(sections[0]!.metaplan?.children).toEqual([]);
    expect(sections[0]!.plans).toEqual(listed);
    expect(sections[2]!.plans).toEqual([alone]);
    expect(rest).toEqual([other]);
  });
});

describe("completeText", () => {
  it("says every plan is complete, or how many of them are", () => {
    expect(completeText([plan("done"), plan("retired")]).text).toBe("2 plans complete");
    expect(completeText([plan("done")]).text).toBe("1 plan complete");
    expect(completeText([plan("done"), plan("written"), plan("running")])).toEqual({
      done: 1,
      total: 3,
      text: "1 of 3 plans complete",
    });
  });
});

describe("railNode", () => {
  it("ranks a gap, then errors, then questions, then the plan's state", () => {
    expect(railNode(entry({ kind: "metaplan_entry", gap: true }), 3, 3)).toBe("gap");
    expect(railNode(plan("done"), 1, 2)).toBe("error");
    expect(railNode(plan("running"), 0, 2)).toBe("question");
    expect(railNode(plan("running"), 0, 0)).toBe("running");
    expect(railNode(plan("done"), 0, 0)).toBe("done");
    expect(railNode(plan("retired"), 0, 0)).toBe("retired");
    expect(railNode(plan("written"), 0, 0)).toBe("written");
  });

  it("opens only the plans that ask for the reader", () => {
    const open = ["gap", "error", "question", "running", "done", "retired", "written"] as const;
    expect(open.filter(needsAttention)).toEqual(["error", "question", "running"]);
  });
});

describe("stageNames", () => {
  it("lists stage folders and loose stage briefs, never the common brief", () => {
    const children = [
      entry({ kind: "brief", file_name: "common.md" }),
      entry({ kind: "brief", file_name: "reading.md" }),
      entry({ kind: "brief_folder", title: "checklist-check" }),
    ];
    expect(stageNames(plan("done", { children }))).toEqual(["reading", "checklist-check"]);
  });
});

describe("questionDocs", () => {
  it("finds every document with open questions, nested ones included, in tree order", () => {
    const brief = entry({ kind: "brief", path: "doc/plans/feat/briefs/a/common.md" });
    const owner = plan("written", { children: [brief] });
    const counts = { "doc/plans/feat/PLAN-written.md": 1, "doc/plans/feat/briefs/a/common.md": 2 };

    expect(questionDocs([owner, plan("done")], counts)).toEqual([
      { entry: owner, count: 1 },
      { entry: brief, count: 2 },
    ]);
  });
});
