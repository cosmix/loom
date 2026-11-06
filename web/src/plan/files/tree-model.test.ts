import { describe, expect, it } from "vitest";

import type { TreeEntry } from "@/plan/api/schema-files";
import {
  arrangeChildren,
  featureCount,
  isFinished,
  entryNumber,
  shortTitle,
} from "@/plan/files/tree-model";

function entry(fields: Partial<TreeEntry> = {}): TreeEntry {
  return {
    feature: "feat-x",
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

const plan = (title: string, state: TreeEntry["state"], file = `PLAN-${title}.md`) =>
  entry({ kind: "plan", title, state, file_name: file, path: `doc/plans/feat-x/${file}` });

const brief = entry({ kind: "brief", title: "Common brief: x", file_name: "common.md" });

describe("arrangeChildren", () => {
  it("lifts a feature's plans beside its metaplan, which becomes a leaf", () => {
    const written = plan("w", "written");
    const done = plan("d", "done");
    const retired = plan("r", "retired");
    const gap = entry({ kind: "metaplan_entry", gap: true, title: "Later" });
    const metaplan = entry({ kind: "metaplan", children: [done, written, retired, gap] });
    const { rows, finished } = arrangeChildren(entry({ kind: "feature", children: [metaplan] }));

    expect(rows.map((row) => row.kind)).toEqual(["metaplan", "plan", "metaplan_entry"]);
    expect(rows[0]!.children).toEqual([]);
    expect(rows[1]).toBe(written);
    expect(finished).toEqual([done, retired]);
  });

  it("keeps a metaplan outside a feature whole, folding only its finished plans", () => {
    const written = plan("w", "written");
    const done = plan("d", "done");
    const metaplan = entry({ kind: "metaplan", feature: null, children: [written, done] });

    expect(arrangeChildren(metaplan)).toEqual({ rows: [written], finished: [done] });
  });

  it("never folds a plan's briefs", () => {
    const owner = plan("w", "done");
    const children = [brief, entry({ kind: "brief_folder", title: "stage" })];

    expect(arrangeChildren({ ...owner, children })).toEqual({ rows: children, finished: [] });
  });

  it("counts done and retired plans as finished, nothing else", () => {
    expect(
      ["written", "running"].map((state) => isFinished(plan("p", state as "written"))),
    ).toEqual([false, false]);
    expect(isFinished(plan("p", "done"))).toBe(true);
    expect(isFinished(plan("p", "retired"))).toBe(true);
    expect(isFinished(entry({ kind: "brief", state: "done" }))).toBe(false);
  });
});

describe("featureCount", () => {
  it("counts finished plans over every listed plan, a gap included", () => {
    const metaplan = entry({
      kind: "metaplan",
      children: [plan("a", "done"), plan("b", "running"), entry({ kind: "metaplan_entry" })],
    });

    expect(featureCount(entry({ kind: "feature", children: [metaplan] }))).toEqual({
      text: "1/3",
      spoken: "1 of 3 plans finished",
    });
  });

  it("counts a product feature's missing documents, and nothing when none is missing", () => {
    const vision = entry({ kind: "vision" });
    const missing = entry({ kind: "section", gap: true });

    expect(featureCount(entry({ kind: "feature", children: [vision, missing] }))?.text).toBe(
      "1 missing",
    );
    expect(featureCount(entry({ kind: "feature", children: [vision] }))).toBeNull();
    expect(featureCount(plan("a", "done"))).toBeNull();
  });
});

describe("shortTitle", () => {
  it.each([
    ["Plan: Document model and checklist", "Document model and checklist"],
    ["Plan – Server core", "Server core"],
    ["plan:  `loom plan` web shell", "`loom plan` web shell"],
    ["Planning sandbox", "Planning sandbox"],
    ["Plan:", "Plan:"],
  ])("drops the plan prefix from %j", (title, expected) => {
    expect(shortTitle(plan(title, "written", "PLAN-feat-x-01-a.md"))).toBe(expected);
  });

  it("shows a plan titled by its slug without the feature or its number", () => {
    const slugged = plan("feat-x-02-server-core", "done", "DONE-PLAN-feat-x-02-server-core.md");
    expect(shortTitle(slugged)).toBe("server-core");
    expect(shortTitle({ ...slugged, feature: null })).toBe("feat-x-02-server-core");
  });

  it("names a feature's metaplan and a common brief by what they are", () => {
    const title = "`loom plan` Engineering Metaplan";
    expect(shortTitle(entry({ kind: "metaplan", title }))).toBe("Metaplan");
    expect(shortTitle(entry({ kind: "metaplan", title, feature: null }))).toBe(title);
    expect(shortTitle(brief)).toBe("Common brief");
    expect(shortTitle(entry({ kind: "vision", title: "Loom Plan — Product Vision" }))).toBe(
      "Vision",
    );
    expect(shortTitle(entry({ kind: "vision", title: "Stray vision", feature: null }))).toBe(
      "Stray vision",
    );
  });

  it("drops the stage word before a stage brief and leaves other titles alone", () => {
    expect(shortTitle(entry({ kind: "brief", title: "Stage reading" }))).toBe("reading");
    expect(shortTitle(entry({ kind: "brief", title: "Stage" }))).toBe("Stage");
    expect(shortTitle(entry({ kind: "section", title: "Plan: kept" }))).toBe("Plan: kept");
  });
});

describe("entryNumber", () => {
  it("reads a product section's number from its file name", () => {
    expect(entryNumber(entry({ kind: "section", file_name: "section_03.md" }))).toBe("03");
    expect(entryNumber(entry({ kind: "section", file_name: "notes.md" }))).toBeNull();
  });

  it("reads a feature plan's ordinal from its file name, whatever the lock prefix", () => {
    expect(entryNumber(plan("A", "done", "DONE-PLAN-feat-x-02-server.md"))).toBe("02");
    expect(entryNumber(plan("A", "written", "PLAN-feat-x-14-live.md"))).toBe("14");
  });

  it("has none for a standalone plan, an unnumbered slug or another feature's slug", () => {
    expect(
      entryNumber({ ...plan("A", "written", "PLAN-feat-x-02-a.md"), feature: null }),
    ).toBeNull();
    expect(entryNumber(plan("A", "written", "PLAN-feat-x-server.md"))).toBeNull();
    expect(entryNumber(plan("A", "written", "PLAN-other-02-a.md"))).toBeNull();
    expect(entryNumber(entry({ kind: "brief", file_name: "PLAN-feat-x-02-a.md" }))).toBeNull();
  });
});
