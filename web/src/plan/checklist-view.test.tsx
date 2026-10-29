import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { authoringViewSchema, checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { ChecklistGaps, NextOffers } from "@/plan/checklist-view";
import { registerOfferAction } from "@/plan/offers/actions";
import { OfferButton } from "@/plan/offers/offer-button";
import { authoringAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]);
const offer: OfferView = checklist.primary!;

function renderWith(ui: React.ReactNode, setup?: (store: ReturnType<typeof createStore>) => void) {
  const store = createStore();
  setup?.(store);
  return render(<Provider store={store}>{ui}</Provider>);
}

describe("ChecklistGaps", () => {
  it("renders the gap text, paths and offers", () => {
    renderWith(<ChecklistGaps checklist={checklist} />);
    expect(screen.getByRole("region", { name: "Gaps" })).toBeTruthy();
    expect(screen.getAllByText("Write section 05").length).toBeGreaterThan(0);
    expect(screen.getByText("section 05: Example")).toBeTruthy();
    expect(screen.getByText("doc/product/demo/section_05.md")).toBeTruthy();
  });

  it("groups gaps under their steps in step order and skips steps without items", () => {
    renderWith(
      <ChecklistGaps
        checklist={{
          ...checklist,
          steps: [...checklist.steps, { number: 3, name: "Engineering spec", mark: "gap" }],
          items: [
            checklist.items[1]!,
            checklist.items[0]!,
            { step: 1, text: "Confirm the vision", path: null, offer: null },
          ],
        }}
      />,
    );
    const gaps = within(screen.getByRole("region", { name: "Gaps" }));
    expect(gaps.getAllByRole("heading", { level: 2 }).map((node) => node.textContent)).toEqual([
      "Step 1 · Vision",
      "Step 2 · Product spec",
    ]);
    const vision = within(gaps.getByRole("region", { name: "Step 1 · Vision" }));
    expect(vision.getAllByRole("listitem").map((node) => node.textContent)).toEqual([
      expect.stringContaining("product vision"),
      expect.stringContaining("Confirm the vision"),
    ]);
    expect(vision.queryByText("section 05: Example")).toBeNull();
    const product = within(gaps.getByRole("region", { name: "Step 2 · Product spec" }));
    expect(product.getAllByRole("listitem")).toHaveLength(1);
    expect(product.getByText("section 05: Example")).toBeTruthy();
    expect(product.getByText("doc/product/demo/section_05.md")).toBeTruthy();
    expect(product.queryByText("product vision")).toBeNull();
  });

  it("never nests an offer button inside an inline span", () => {
    const unregister = registerOfferAction(offer.kind, vi.fn());
    try {
      renderWith(
        <>
          <NextOffers checklist={checklist} />
          <ChecklistGaps checklist={checklist} />
        </>,
      );
      const buttons = screen.getAllByRole("button", { name: offer.label });
      expect(buttons).toHaveLength(2);
      for (const button of buttons) expect(button.closest("span")).toBeNull();
    } finally {
      unregister();
    }
  });

  it("makes control characters visible in server step names, gap text and paths", () => {
    renderWith(
      <ChecklistGaps
        checklist={{
          ...checklist,
          steps: [{ number: 1, name: "Vis\u202Eion", mark: "gap" }],
          items: [{ step: 1, text: "Re\u202Eview", path: "doc/vis\u202Eion.md", offer: null }],
        }}
      />,
    );

    expect(screen.getByRole("region", { name: "Step 1 · Vis⟨U+202E⟩ion" })).toBeTruthy();
    expect(screen.getByText("Re⟨U+202E⟩view")).toBeTruthy();
    expect(screen.getByText("doc/vis⟨U+202E⟩ion.md")).toBeTruthy();
  });
});

describe("NextOffers", () => {
  it("renders the primary and secondary offers in order", () => {
    renderWith(<NextOffers checklist={checklist} />);

    const next = within(screen.getByRole("region", { name: "Next" }));
    expect(next.getByText("Write section 05")).toBeTruthy();
    expect(next.getByText("Start a metaplan from the product spec")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Next" }).textContent).toBe(
      "Write section 05Start a metaplan from the product spec",
    );
  });

  it("renders secondary offers without a primary offer", () => {
    renderWith(<NextOffers checklist={{ ...checklist, primary: null }} />);

    const next = within(screen.getByRole("region", { name: "Next" }));
    expect(next.queryByText("Write section 05")).toBeNull();
    expect(next.getByText("Start a metaplan from the product spec")).toBeTruthy();
  });

  it("renders nothing when there are no offers", () => {
    const { container } = renderWith(
      <NextOffers checklist={{ ...checklist, primary: null, secondary: [] }} />,
    );

    expect(container.childElementCount).toBe(0);
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
