import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type CandidateView, type ChecklistView } from "@/plan/api/schema";
import {
  featureStatus,
  followUpFeatures,
  followUpSeed,
  PLAN_FOLLOW_UPS_ID,
} from "@/plan/new/follow-up-features";

const metaplans = [{ path: "doc/plans/x/index.md", title: "Roadmap" }];
const candidate: CandidateView = {
  id: "F-1",
  kind: "finding",
  stage: "Chat polish",
  stage_id: "chat-polish",
  text: "Settings statistics look bland",
  review: "doc/plans/REVIEW-PLAN-x-01.md",
};

function feature(name: string, states: string[], patch: Record<string, unknown> = {}) {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: name,
    metaplans,
    current: { kind: "ready" },
    plan_rows: states.map((state, index) => ({
      slug: `${name}-${index}`,
      path: `doc/plans/${name}/PLAN-${name}-${index}.md`,
      title: `Plan ${index}`,
      state,
      pressure_date: null,
      listed: true,
    })),
    ...patch,
  });
}

const finished = (name: string, count: number): ChecklistView =>
  feature(name, Array<string>(count).fill("done"), {
    current: { kind: "ready_all_done" },
    candidates: [candidate],
  });

it("names the follow-up offer plan-follow-ups", () => {
  expect(PLAN_FOLLOW_UPS_ID).toBe("plan-follow-ups");
});

it("states a built, an unfinished and an empty feature", () => {
  expect(featureStatus(true, 13, 13)).toBe("done · 13 plans");
  expect(featureStatus(true, 1, 1)).toBe("done · 1 plan");
  expect(featureStatus(false, 4, 6)).toBe("in progress · 4 of 6 plans");
  expect(featureStatus(false, 0, 1)).toBe("in progress · 0 of 1 plan");
  expect(featureStatus(false, 0, 0)).toBe("no plans yet");
});

it("lists every feature with a metaplan, in order, and leaves out one without", () => {
  const list = followUpFeatures([
    finished("atlas", 2),
    feature("beacon", ["done", "written", "retired"]),
    feature("comet", [], { metaplans: [] }),
  ]);

  expect(list.map((entry) => entry.name)).toEqual(["atlas", "beacon"]);
  expect(list[0]).toEqual({
    name: "atlas",
    built: true,
    done: 2,
    total: 2,
    status: "done · 2 plans",
    candidates: [candidate],
  });
  expect(list[1]).toEqual({
    name: "beacon",
    built: false,
    done: 1,
    total: 2,
    status: "in progress · 1 of 2 plans",
    candidates: [],
  });
});

it("reads a feature with a metaplan and no plan as having no plans yet", () => {
  const [entry] = followUpFeatures([feature("comet", [])]);

  expect(entry).toMatchObject({ built: false, total: 0, status: "no plans yet" });
});

it("is built only when the server says all done and every live plan is done", () => {
  const built = (kind: "ready" | "ready_all_done", states: string[]) =>
    followUpFeatures([feature("atlas", states, { current: { kind } })])[0]!.built;

  expect(built("ready_all_done", ["done", "done"])).toBe(true);
  expect(built("ready_all_done", ["done", "written"])).toBe(false);
  expect(built("ready", ["done", "done"])).toBe(false);
  expect(built("ready_all_done", ["done", "retired"])).toBe(true);
  expect(built("ready_all_done", ["retired"])).toBe(false);
  expect(built("ready_all_done", [])).toBe(false);
});

it("seeds a follow-up with the candidate, its kind, review file and stage id", () => {
  expect(followUpSeed(candidate)).toBe(
    "F-1: Settings statistics look bland (open finding from REVIEW-PLAN-x-01.md, stage chat-polish)",
  );
  expect(followUpSeed({ ...candidate, kind: "question" })).toBe(
    "F-1: Settings statistics look bland (open question from REVIEW-PLAN-x-01.md, stage chat-polish)",
  );
});

it("seeds a hand-written finding without a stage, and only its first line", () => {
  const byHand = {
    ...candidate,
    id: "F-7",
    stage: "review",
    stage_id: "",
    text: "Found by hand\nsecond line",
    review: "doc/plans/x/findings.md",
  };
  expect(followUpSeed(byHand)).toBe("F-7: Found by hand (open finding from findings.md)");
});
