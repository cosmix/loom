import { expect, it } from "vitest";

import type { TreeEntry } from "@/plan/api/schema-files";
import { planPath } from "./turn-footer-plans";

function entry(path: string | null, children: TreeEntry[] = []): TreeEntry {
  return {
    feature: null,
    misplaced: false,
    path,
    title: path ?? "folder",
    file_name: path?.split("/").at(-1) ?? null,
    kind: "plan",
    state: null,
    locked: false,
    gap: false,
    pressure_date: null,
    errors: 0,
    blurb: null,
    children,
  };
}

it("finds the plan by its slug under any state prefix", () => {
  const tree = [
    entry(null, [entry("doc/plans/PLAN-other.md"), entry("doc/plans/IN_PROGRESS-PLAN-feat.md")]),
  ];
  expect(planPath(tree, "feat")).toBe("doc/plans/IN_PROGRESS-PLAN-feat.md");
  expect(planPath(tree, "other")).toBe("doc/plans/PLAN-other.md");
  expect(planPath(tree, "missing")).toBeUndefined();
});

it("never returns the run review of the plan", () => {
  const tree = [
    entry("doc/plans/archive/REVIEW-PLAN-feat.md"),
    entry("doc/plans/archive/REVIEW-IN_PROGRESS-PLAN-feat.md"),
    entry("doc/plans/archive/DONE-PLAN-feat.md"),
  ];
  expect(planPath(tree, "feat")).toBe("doc/plans/archive/DONE-PLAN-feat.md");
  expect(planPath(tree.slice(0, 2), "feat")).toBeUndefined();
});
