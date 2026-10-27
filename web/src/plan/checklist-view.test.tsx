import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { authoringViewSchema, checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { ChecklistView } from "@/plan/checklist-view";
import { registerOfferAction } from "@/plan/offers/actions";
import { OfferButton } from "@/plan/offers/offer-button";
import { authoringAtom, checklistAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const checklist = checklistViewSchema.parse(checklistFixture);
const offer: OfferView = checklist.primary!;

function renderWith(ui: React.ReactNode, setup?: (store: ReturnType<typeof createStore>) => void) {
  const store = createStore();
  setup?.(store);
  return render(<Provider store={store}>{ui}</Provider>);
}

describe("ChecklistView", () => {
  it("renders the current step, the primary offer and the gaps", () => {
    renderWith(<ChecklistView />, (store) => store.set(checklistAtom, checklist));
    expect(screen.getByRole("heading", { name: "Step 2 of 2: Product spec" })).toBeTruthy();
    expect(screen.getByRole("listitem", { current: "step" }).textContent).toContain("Product spec");
    expect(screen.getAllByText("Write section 05").length).toBeGreaterThan(0);
    expect(screen.getByText("section 05: Example")).toBeTruthy();
    expect(screen.getByText("doc/product/section_05.md")).toBeTruthy();
  });

  it("shows the loading state before the first checklist", () => {
    renderWith(<ChecklistView />);
    expect(screen.getByRole("status", { name: "Loading the checklist" })).toBeTruthy();
  });

  it("names the ready state when no step is current", () => {
    renderWith(<ChecklistView />, (store) =>
      store.set(checklistAtom, { ...checklist, current: { kind: "ready" } }),
    );
    expect(screen.getByRole("heading", { name: "Ready to run" })).toBeTruthy();
    expect(screen.queryByRole("listitem", { current: "step" })).toBeNull();
  });
});

describe("OfferButton", () => {
  it("reads as text until a handler is registered, then as a button", () => {
    const handler = vi.fn();
    const { rerender } = renderWith(<OfferButton offer={offer} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText(offer.label)).toBeTruthy();
    const unregister = registerOfferAction(offer.kind, handler);
    rerender(<OfferButton offer={offer} />);
    fireEvent.click(screen.getByRole("button", { name: offer.label }));
    expect(handler).toHaveBeenCalledWith(offer);
    unregister();
    rerender(<OfferButton offer={offer} />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("describes the button with the holder's name when another browser holds authoring", () => {
    const unregister = registerOfferAction(offer.kind, vi.fn());
    renderWith(<OfferButton offer={offer} />, (store) => {
      store.set(authoringAtom, authoringViewSchema.parse(authoringFixture));
    });
    const holder = authoringViewSchema.parse(authoringFixture).holder!;
    expect(
      screen.getByRole("button", {
        name: offer.label,
        description: `${holder.name} holds authoring`,
      }),
    ).toBeTruthy();
    unregister();
  });
});
