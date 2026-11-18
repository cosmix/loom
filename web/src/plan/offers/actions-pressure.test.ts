import { createStore } from "jotai";
import { describe, expect, it } from "vitest";
import type { OfferView } from "@/plan/api/schema";
import { runOffer } from "@/plan/offers/actions";
import { pressureDialogAtom, registerPressureOfferAction } from "@/plan/offers/actions-pressure";

const offer: OfferView = {
  id: "offer-1",
  kind: "pressure_test",
  label: "Pressure test",
  blocked_reason: null,
  target: "doc/plans/example.md",
};

describe("pressure offer action", () => {
  it("opens the dialog with the offer target and unregisters the action", () => {
    const store = createStore();
    const unregister = registerPressureOfferAction(store);

    try {
      expect(store.get(pressureDialogAtom)).toEqual({ open: false, plan: null });
      expect(runOffer(offer)).toBe(true);
      expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: "doc/plans/example.md" });
    } finally {
      unregister();
    }

    expect(runOffer(offer)).toBe(false);
  });
});
