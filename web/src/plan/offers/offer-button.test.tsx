import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { authoringViewSchema, checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { OfferButton } from "@/plan/offers/offer-button";
import { authoringAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const offer: OfferView = checklistViewSchema.parse(checklistFixture.checklists[0]).primary!;

function renderWith(ui: React.ReactNode, setup?: (store: ReturnType<typeof createStore>) => void) {
  const store = createStore();
  setup?.(store);
  return render(<Provider store={store}>{ui}</Provider>);
}

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

  it("spells out control characters in the label of an offer with no action", () => {
    renderWith(<OfferButton offer={{ ...offer, label: "Write section 05\u202E" }} />);
    expect(screen.getByText("Write section 05⟨U+202E⟩")).toBeTruthy();
  });

  it("spells out control characters in the label of an offer button", () => {
    const unregister = registerOfferAction(offer.kind, vi.fn());
    try {
      renderWith(<OfferButton offer={{ ...offer, label: "Write section 05\u202E" }} />);
      expect(screen.getByRole("button", { name: "Write section 05⟨U+202E⟩" })).toBeTruthy();
    } finally {
      unregister();
    }
  });
});
