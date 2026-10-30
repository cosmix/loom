import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { PlanRunActions } from "@/plan/run/plan-run-actions";
import { checklistAtom } from "@/plan/state/atoms";

import { authorStore } from "../doc-heading-kit";

const PATH = "doc/plans/demo/PLAN-file-mode.md";

afterEach(cleanup);

function offer(kind: OfferView["kind"], label: string): OfferView {
  return { id: `${kind}:demo`, kind, label, target: PATH, feature: "demo" };
}

function mountDone(...offers: OfferView[]) {
  const store = authorStore();
  const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
  store.set(checklistAtom, {
    ...checklist,
    primary: offers[0] ?? null,
    secondary: offers.slice(1),
  });
  return render(
    <Provider store={store}>
      <PlanRunActions path={PATH} state="done" />
    </Provider>,
  );
}

it("renders nothing for a done plan the server makes no offer for", () => {
  const { container } = mountDone();

  expect(container.firstChild).toBeNull();
  expect(screen.queryByText("No offer is available")).toBeNull();
});

it("renders only the follow-up the server offers, and runs its handler", () => {
  const review = offer("refine_spec", "Review demo");
  const handler = vi.fn();
  const unregister = registerOfferAction("refine_spec", handler);
  try {
    mountDone(review);

    expect(screen.queryByRole("button", { name: "Start a new metaplan round" })).toBeNull();
    expect(screen.queryByText("No offer is available")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Review what was built" }));
    expect(handler).toHaveBeenCalledWith(review);
  } finally {
    unregister();
  }
});

it("renders both follow-ups when both are offered", () => {
  mountDone(offer("refine_spec", "Review demo"), offer("new_metaplan_round", "Round demo"));

  expect(screen.getByRole("button", { name: "Review what was built" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Start a new metaplan round" })).toBeTruthy();
});
