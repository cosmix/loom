import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import { pressureOptionsSchema } from "@/plan/api/schema-pressure";
import { pressureOptionsAtom } from "@/plan/chat/pressure/use-pressure";
import { authoringAtom, checklistAtom } from "@/plan/state/atoms";
import { PlanRunActions } from "./plan-run-actions";

const path = "doc/plans/PLAN-example.md";
afterEach(cleanup);

it("opens the dialogs for a written plan and respects authoring restrictions", () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pressure-test" }));
  fireEvent.click(screen.getByRole("button", { name: "Start a run" }));
  expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: path });
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: path });
  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "other", name: "Ada", away_since: null, frees_at: null },
    }),
  );
  expect(
    screen.getByRole("button", { name: "Pressure-test", description: "Ada holds authoring" }),
  ).toBeTruthy();
});

it("disables Pressure-test with the server reason when the options say unavailable", () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: pressureOptionsSchema.parse({
      ...optionsFixture,
      available: false,
      disabled_reason: "Codex is not installed",
    }),
  });
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pressure-test" }));
  expect(
    screen.getByRole("button", { name: "Pressure-test", description: "Codex is not installed" }),
  ).toBeTruthy();
  expect(store.get(pressureDialogAtom).open).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Start a run" }));
  expect(store.get(runDialogAtom).open).toBe(true);
});

it("dispatches the real checklist offers for a done plan", () => {
  const store = createStore();
  const review: OfferView = { id: "review-id", kind: "refine_spec", label: "Review", target: null };
  const round: OfferView = {
    id: "round-id",
    kind: "new_metaplan_round",
    label: "Round",
    target: path,
  };
  store.set(checklistAtom, {
    ...checklistViewSchema.parse(fixture),
    primary: review,
    secondary: [round],
  });
  const handler = vi.fn();
  const unregisterReview = registerOfferAction("refine_spec", handler);
  const unregisterRound = registerOfferAction("new_metaplan_round", handler);
  try {
    render(
      <Provider store={store}>
        <PlanRunActions path={path} state="done" />
      </Provider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Review what was built" }));
    fireEvent.click(screen.getByRole("button", { name: "Start a new metaplan round" }));
    expect(handler).toHaveBeenNthCalledWith(1, review);
    expect(handler).toHaveBeenNthCalledWith(2, round);
  } finally {
    unregisterReview();
    unregisterRound();
  }
});

it("explains when a done-plan offer is unavailable", () => {
  render(
    <Provider store={createStore()}>
      <PlanRunActions path={path} state="done" />
    </Provider>,
  );
  expect(
    screen.getByRole("button", {
      name: "Review what was built",
      description: "No offer is available",
    }),
  ).toBeTruthy();
  expect(
    screen.getByRole("button", {
      name: "Start a new metaplan round",
      description: "No offer is available",
    }),
  ).toBeTruthy();
});

it.each(["running", "retired"] as const)("renders no actions for a %s plan", (state) => {
  const { container } = render(
    <Provider store={createStore()}>
      <PlanRunActions path={path} state={state} />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");
});
