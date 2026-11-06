import { expect, it } from "vitest";

import { atlas, offer, planRow } from "@/plan/feature/pages-kit";
import { pageDrawsPrimary } from "@/plan/feature/page-offer";

const path = (slug: string) => `doc/plans/atlas/PLAN-${slug}.md`;
const test = offer("pressure_test", "Pressure-test gateway", path("gateway"), "pt:gateway");
const item = (step: number, forPath: string, entry = test) => ({
  step,
  text: entry.label,
  path: forPath,
  offer: entry,
});

const pressure = atlas({
  plan_rows: [planRow("ledger", "written", "2026-10-20"), planRow("gateway", "written")],
  primary: test,
  items: [item(5, path("gateway"))],
});

it("leaves a pressure test to the spine on every page but its plan's own", () => {
  expect(pageDrawsPrimary(pressure, "/f/atlas/pressure")).toBe(false);
  expect(pageDrawsPrimary(pressure, "/f/atlas/run")).toBe(false);
  expect(pageDrawsPrimary(pressure, "/f/atlas/engineering")).toBe(false);
  const owned = { ...pressure, primary: { ...test, slug: "gateway" } };
  expect(pageDrawsPrimary(owned, "/f/atlas/engineering/gateway")).toBe(true);
  expect(pageDrawsPrimary(owned, "/f/atlas/engineering/ledger")).toBe(false);
});

it("owns the primary start_run on the Run page, whatever the plans' states", () => {
  const start = offer("start_run", "Start a run", path("gateway"));
  const run = atlas({ plan_rows: [planRow("gateway", "written")], primary: start });
  expect(pageDrawsPrimary(run, "/f/atlas/run")).toBe(true);
  expect(pageDrawsPrimary(run, "/f/atlas/engineering")).toBe(false);
  // The Run page's button resumes a stopped run of a plan that is no longer written, blocks while a
  // run holds the workspace, and so is the offer's one owner in every state (`startControl`).
  const running = atlas({ plan_rows: [planRow("gateway", "running")], primary: start });
  expect(pageDrawsPrimary(running, "/f/atlas/run")).toBe(true);
  expect(pageDrawsPrimary({ ...run, primary: null }, "/f/atlas/run")).toBe(false);
});

it("owns the primary review_feature on the Findings page only", () => {
  const review = offer("review_feature", "Review the feature");
  const done = atlas({ plan_rows: [planRow("gateway", "done")], primary: review });
  expect(pageDrawsPrimary(done, "/f/atlas/findings")).toBe(true);
  expect(pageDrawsPrimary(done, "/f/atlas/run")).toBe(false);
  expect(pageDrawsPrimary(done, "/f/atlas/coverage")).toBe(false);
  // Another primary stays with the spine on the Findings page: the page draws only its own offer.
  const start = offer("start_run", "Start a run", path("gateway"));
  expect(pageDrawsPrimary({ ...done, primary: start }, "/f/atlas/findings")).toBe(false);
});
