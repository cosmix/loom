import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import {
  pressureOptionsSchema,
  pressureRunSchema,
  type PressureRunView,
} from "@/plan/api/schema-pressure";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";

import { PressureDone } from "./pressure-done";
import { pressureOptionsAtom } from "./use-pressure";

const plan = "doc/plans/PLAN-example.md";
const nextPlan = "doc/plans/PLAN-next.md";
const brief = "doc/plans/briefs/example/ui notes.md";
const options = pressureOptionsSchema.parse(optionsFixture);

function recordedRun(): PressureRunView {
  const run = pressureRunSchema.parse(runFixture);
  return {
    ...run,
    tokens_total: 12345,
    steps: [
      { ...run.steps[0]!, ended_ms: 31000, edits: [plan, brief, plan] },
      { ...run.steps[1]!, state: "done", ended_ms: 66999, edits: [brief, plan] },
    ],
    ended: { kind: "recorded", by: null, step: null, reason: null, line: "Recorded" },
  };
}

function LocationProbe() {
  const location = useLocation();
  return <output aria-label="Current address">{location.pathname + location.search}</output>;
}

function mount(run = recordedRun(), plans = [plan, nextPlan, "doc/plans/PLAN-later.md"]) {
  const store = createStore();
  store.set(pressureOptionsAtom, { ok: true, value: { ...options, plans } });
  const view = render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureDone run={run} />
        <LocationProbe />
      </MemoryRouter>
    </Provider>,
  );
  return { ...view, store };
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it.each([null, "stopped", "failed", "interrupted"] as const)(
  "renders no summary when the run end is %s",
  (kind) => {
    const run = recordedRun();
    run.ended = kind === null ? null : { ...run.ended!, kind };

    mount(run);

    expect(screen.queryByRole("region", { name: "Pressure test summary" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Test the next plan" })).toBeNull();
    expect(screen.queryByRole("link", { name: "Review the changes" })).toBeNull();
  },
);

it("shows rounds, elapsed whole seconds and compact total tokens", () => {
  mount();

  const summary = within(screen.getByRole("region", { name: "Pressure test summary" }));
  expect(summary.getByText("2 rounds")).toBeTruthy();
  expect(summary.getByText("1m5s")).toBeTruthy();
  expect(summary.getByText("12.3k tokens")).toBeTruthy();
});

it("uses the last ended step when a trailing step has no end timestamp", () => {
  const run = recordedRun();
  run.steps[1]!.ended_ms = null;

  mount(run);

  expect(screen.getByText("30s")).toBeTruthy();
});

it.each(["empty", "unfinished"])(
  "counts to the moment the end arrived with %s steps, as the header does",
  (kind) => {
    const run = recordedRun();
    run.steps = kind === "empty" ? [] : run.steps.map((step) => ({ ...step, ended_ms: null }));
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(run.started_ms + 90_000);

    mount(run);

    expect(screen.getByText("1m30s")).toBeTruthy();
  },
);

it("shows a singular round and clamps an end before the start to zero", () => {
  const run = recordedRun();
  run.rounds = 1;
  run.steps = [{ ...run.steps[0]!, ended_ms: 0 }];

  mount(run);

  expect(screen.getByText("1 round")).toBeTruthy();
  expect(screen.getByText("0s")).toBeTruthy();
});

it("lists each edited file once with its document and HEAD diff links", () => {
  mount();

  const list = within(screen.getByRole("list", { name: "Edited files" }));
  const rows = list.getAllByRole("listitem");
  expect(rows).toHaveLength(2);
  expect(within(rows[0]!).getByRole("link", { name: plan }).getAttribute("href")).toBe(
    "/files/plans/PLAN-example.md",
  );
  expect(within(rows[0]!).getByRole("link", { name: "Diff" }).getAttribute("href")).toBe(
    "/files/plans/PLAN-example.md?diff=head",
  );
  expect(within(rows[1]!).getByRole("link", { name: brief }).getAttribute("href")).toBe(
    "/files/plans/briefs/example/ui%20notes.md",
  );
  expect(within(rows[1]!).getByRole("link", { name: "Diff" }).getAttribute("href")).toBe(
    "/files/plans/briefs/example/ui%20notes.md?diff=head",
  );
});

it("makes file control characters visible while preserving the encoded link", () => {
  const run = recordedRun();
  run.steps = [{ ...run.steps[0]!, edits: ["doc/plans/PLAN-\u202Ehidden.md"] }];

  mount(run);

  const link = screen.getByRole("link", { name: "doc/plans/PLAN-⟨U+202E⟩hidden.md" });
  expect(link.getAttribute("href")).toBe("/files/plans/PLAN-%E2%80%AEhidden.md");
  expect(link.textContent).not.toContain("\u202E");
});

it("opens the plan's HEAD diff from Review the changes", () => {
  mount();
  const review = screen.getByRole("link", { name: "Review the changes" });
  expect(review.getAttribute("href")).toBe("/files/plans/PLAN-example.md?diff=head");

  fireEvent.click(review);

  expect(screen.getByLabelText("Current address").textContent).toBe(
    "/files/plans/PLAN-example.md?diff=head",
  );
});

it("opens the pressure dialog with the first other available plan", () => {
  const { store } = mount();

  fireEvent.click(screen.getByRole("button", { name: "Test the next plan" }));

  expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: nextPlan });
});

it.each([{ plans: [plan] }, { plans: [] }])(
  "blocks testing the next plan, with its reason, when plans are $plans",
  ({ plans }) => {
    const { store } = mount(recordedRun(), plans);
    const button = screen.getByRole("button", { name: "Test the next plan" }) as HTMLButtonElement;

    fireEvent.click(button);

    expect(button.disabled).toBe(false);
    expect(button.getAttribute("aria-disabled")).toBe("true");
    const reason = screen.getByText("There is no other plan to pressure-test.");
    expect(button.getAttribute("aria-describedby")).toBe(reason.id);
    expect(store.get(pressureDialogAtom)).toEqual({ open: false, plan: null });
  },
);
