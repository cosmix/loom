import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { atlas, offer, planRow, renderPage, runState } from "@/plan/feature/pages-kit";
import { RunPage } from "@/plan/feature/run-page";
import { runDialogAtom } from "@/plan/offers/actions-run";

const PATH = "/f/atlas/run";
const ONE = "doc/plans/atlas/PLAN-one.md";
const FOREIGN = "doc/plans/other/PLAN-z.md";
const HELD = "A run holds the workspace.";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const start = () => screen.getAllByRole("button", { name: "Start a run" });
const only = () => {
  expect(start()).toHaveLength(1);
  return start()[0]!;
};

/** One plan whose run is up, and another still to write: the rows the stopped cases need. */
const held = atlas({ plan_rows: [planRow("one", "running"), planRow("two", "written")] });
const resume = (patch: Record<string, unknown> = {}) =>
  runState({ workspace: "stopped", active_plan: ONE, offer: "resume", preselected: ONE, ...patch });

it("resumes a stopped run of this feature on its own plan, not on another written plan", () => {
  const { store } = renderPage(<RunPage />, PATH, held, { holder: "me", run: resume() });

  expect(only().getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(only());
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: ONE });
});

it("draws the resume control for a stopped run even when no plan is written", () => {
  const checklist = atlas({ plan_rows: [planRow("one", "running")] });
  const { store } = renderPage(<RunPage />, PATH, checklist, { holder: "me", run: resume() });

  fireEvent.click(only());
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: ONE });
});

it("blocks Start with one reason while a run of this feature is running", () => {
  const running = runState({ workspace: "running", active_plan: ONE, offer: "none_running" });
  renderPage(<RunPage />, PATH, held, { run: running });

  expect(only().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText(HELD)).toHaveLength(1);
  expect(screen.queryByText(/Start a chat to take authoring/)).toBeNull();
});

it("blocks Start while another plan's run is running, and while it is stopped", () => {
  const running = runState({ workspace: "running", active_plan: FOREIGN, offer: "none_running" });
  const first = renderPage(<RunPage />, PATH, held, { holder: "me", run: running });
  expect(only().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText(HELD)).toHaveLength(1);
  first.unmount();

  const stopped = resume({ active_plan: FOREIGN, preselected: FOREIGN });
  renderPage(<RunPage />, PATH, held, { holder: "me", run: stopped });
  expect(only().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText(HELD)).toHaveLength(1);
});

it("keeps a blocked Start the page's primary when the checklist's primary is start_run", () => {
  const checklist = atlas({
    plan_rows: [planRow("two", "written")],
    primary: offer("start_run", "Start a run", "doc/plans/atlas/PLAN-two.md"),
  });
  const running = runState({ workspace: "running", active_plan: FOREIGN, offer: "none_running" });
  renderPage(<RunPage />, PATH, checklist, { holder: "me", run: running });

  expect(only().getAttribute("aria-disabled")).toBe("true");
  expect(only().getAttribute("data-look")).toBe("primary");
});

it("keeps Start enabled for a workspace that is set up or done", () => {
  const checklist = atlas({ plan_rows: [planRow("two", "written")] });
  const initialized = runState({ workspace: "initialized", active_plan: FOREIGN, offer: "start" });
  const first = renderPage(<RunPage />, PATH, checklist, { holder: "me", run: initialized });
  expect(only().getAttribute("aria-disabled")).toBeNull();
  first.unmount();

  renderPage(<RunPage />, PATH, checklist, {
    holder: "me",
    run: runState({ workspace: "done", active_plan: null, offer: "start" }),
  });
  expect(only().getAttribute("aria-disabled")).toBeNull();
});
