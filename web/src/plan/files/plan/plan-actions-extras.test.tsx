import { cleanup, render, screen } from "@testing-library/react";
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

// The follow-ups belong to the feature's Run page (feature/run-page.test.tsx), not to a plan's header.
it("draws no follow-up for a done plan, even when the server offers both", () => {
  const review = offer("refine_spec", "Review demo");
  const handler = vi.fn();
  const unregister = registerOfferAction("refine_spec", handler);
  try {
    const { container } = mountDone(review, offer("new_metaplan_round", "Round demo"));

    expect(container.firstChild).toBeNull();
    expect(screen.queryByRole("button", { name: "Compare spec with build" })).toBeNull();
    expect(screen.queryByRole("button", { name: "New metaplan round" })).toBeNull();
    expect(handler).not.toHaveBeenCalled();
  } finally {
    unregister();
  }
});
