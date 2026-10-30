import { expect, it } from "vitest";

import { atlas, offer, planRow } from "@/plan/feature/pages-kit";
import { pageDrawsPrimary, pressureOffer } from "@/plan/feature/page-offer";

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

it("finds the step-5 offer only for a written plan with no pressure test on record", () => {
  const [ledger, gateway] = pressure.plan_rows;
  expect(pressureOffer(pressure, ledger!)).toBeNull();
  expect(pressureOffer(pressure, gateway!)?.id).toBe("pt:gateway");
  expect(pressureOffer(pressure, { ...gateway!, state: "done" })).toBeNull();
  expect(pressureOffer({ ...pressure, items: [item(4, path("gateway"))] }, gateway!)).toBeNull();
  expect(pressureOffer({ ...pressure, items: [item(5, path("ledger"))] }, gateway!)).toBeNull();
});

it("owns the primary pressure test on the Pressure page only when a row offers it", () => {
  expect(pageDrawsPrimary(pressure, "/f/atlas/pressure")).toBe(true);
  expect(pageDrawsPrimary(pressure, "/f/atlas/run")).toBe(false);
  expect(pageDrawsPrimary({ ...pressure, items: [] }, "/f/atlas/pressure")).toBe(false);
  const other = offer("pressure_test", "Pressure-test other", path("other"), "pt:other");
  expect(pageDrawsPrimary({ ...pressure, primary: other }, "/f/atlas/pressure")).toBe(false);
});

it("owns the primary start_run on the Run page, whatever the plans' states", () => {
  const start = offer("start_run", "Start a run", path("gateway"));
  const run = atlas({ plan_rows: [planRow("gateway", "written")], primary: start });
  expect(pageDrawsPrimary(run, "/f/atlas/run")).toBe(true);
  expect(pageDrawsPrimary(run, "/f/atlas/pressure")).toBe(false);
  // The Run page's button resumes a stopped run of a plan that is no longer written, blocks while a
  // run holds the workspace, and so is the offer's one owner in every state (`startControl`).
  const running = atlas({ plan_rows: [planRow("gateway", "running")], primary: start });
  expect(pageDrawsPrimary(running, "/f/atlas/run")).toBe(true);
  expect(pageDrawsPrimary({ ...run, primary: null }, "/f/atlas/run")).toBe(false);
});
