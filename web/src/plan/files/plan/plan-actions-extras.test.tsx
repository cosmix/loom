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

// The follow-ups offer belongs to the feature, not to a plan's header.
it("draws no follow-up for a done plan, even when the server offers it", () => {
  const followUps = offer("plan_follow_ups", "Plan follow-ups");
  const handler = vi.fn();
  const unregister = registerOfferAction("plan_follow_ups", handler);
  try {
    const { container } = mountDone(followUps);

    expect(container.firstChild).toBeNull();
    expect(screen.queryByRole("button", { name: "Plan follow-ups" })).toBeNull();
    expect(handler).not.toHaveBeenCalled();
  } finally {
    unregister();
  }
});
