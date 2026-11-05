import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type CandidateView, type ChecklistView } from "@/plan/api/schema";
import {
  builtOf,
  EMPTY_BUILT,
  followUpSeed,
  isBuilt,
  reviewFile,
  reviewHref,
} from "@/plan/feature/built-model";

const row = (slug: string, state: string) => ({
  slug,
  path: `doc/plans/atlas/PLAN-${slug}.md`,
  title: slug,
  state,
  pressure_date: null,
  listed: true,
});

function feature(kind: "ready" | "ready_all_done", states: string[]): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    current: { kind },
    plan_rows: states.map((state, index) => row(`p${index}`, state)),
  });
}

it("is built when the server says all done and every live plan is done", () => {
  expect(isBuilt(feature("ready_all_done", ["done", "done"]))).toBe(true);
});

it("is not built while a plan is still written", () => {
  expect(isBuilt(feature("ready_all_done", ["done", "written"]))).toBe(false);
});

it("is not built when the server does not say all done", () => {
  expect(isBuilt(feature("ready", ["done", "done"]))).toBe(false);
});

it("does not count a retired plan, and is not built with no live plan", () => {
  expect(isBuilt(feature("ready_all_done", ["done", "retired"]))).toBe(true);
  expect(isBuilt(feature("ready_all_done", ["retired"]))).toBe(false);
  expect(isBuilt(feature("ready_all_done", []))).toBe(false);
});

it("reads the built view of a checklist, or the empty one", () => {
  const built = {
    reviews: [{ slug: "atlas-01", path: "doc/plans/REVIEW-PLAN-atlas-01.md" }],
    spec: [],
    candidates: [],
  };
  const withBuilt = checklistViewSchema.parse({ ...feature("ready_all_done", ["done"]), built });
  expect(builtOf(withBuilt)).toEqual(built);
  expect(builtOf(feature("ready_all_done", ["done"]))).toBe(EMPTY_BUILT);
});

it("names a review by its last path segment", () => {
  expect(reviewFile("doc/plans/atlas/archive/REVIEW-PLAN-x.md")).toBe("REVIEW-PLAN-x.md");
  expect(reviewFile("REVIEW-PLAN-x.md")).toBe("REVIEW-PLAN-x.md");
});

it("opens a standalone review in the file browser and a feature's inside its feature", () => {
  expect(reviewHref("doc/plans/REVIEW-PLAN-x.md")).toBe("/files/plans/REVIEW-PLAN-x.md");
  expect(reviewHref("doc/plans/atlas/archive/REVIEW-PLAN-x.md")).toBe(
    "/f/atlas/source/plans/atlas/archive/REVIEW-PLAN-x.md",
  );
});

const candidate = (kind: CandidateView["kind"]): CandidateView => ({
  kind,
  stage: "Chat polish",
  stage_id: "chat-polish",
  text: "Settings statistics look bland",
  review: "doc/plans/REVIEW-PLAN-atlas-01.md",
});

it("seeds a follow-up with the candidate, its kind, review file and stage id", () => {
  expect(followUpSeed(candidate("finding"))).toBe(
    "Settings statistics look bland (open finding from REVIEW-PLAN-atlas-01.md, stage chat-polish)",
  );
  expect(followUpSeed(candidate("question"))).toBe(
    "Settings statistics look bland (open question from REVIEW-PLAN-atlas-01.md, stage chat-polish)",
  );
});
