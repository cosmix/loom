import { createStore } from "jotai";
import { describe, expect, it } from "vitest";
import type { OfferView } from "@/plan/api/schema";
import { runOffer } from "@/plan/offers/actions";
import { registerRunOfferAction, runDialogAtom } from "@/plan/offers/actions-run";

const offer: OfferView = {
  id: "offer-1",
  kind: "start_run",
  label: "Start run",
  target: "doc/plans/example.md",
};

describe("run offer action", () => {
  it("opens the dialog with the offer target and unregisters the action", () => {
    const store = createStore();
    const unregister = registerRunOfferAction(store);

    try {
      expect(store.get(runDialogAtom)).toEqual({ open: false, plan: null });
      expect(runOffer(offer)).toBe(true);
      expect(store.get(runDialogAtom)).toEqual({ open: true, plan: "doc/plans/example.md" });
    } finally {
      unregister();
    }

    expect(runOffer(offer)).toBe(false);
  });
});
