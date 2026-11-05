// Frozen contract: the Plan follow-ups offer kind has an authoring handler, so its button
// starts an authoring chat instead of doing nothing.
import { expect, it, vi } from "vitest";
import type { OfferView } from "@/plan/api/schema";
import { runOffer } from "@/plan/offers/actions";
import { registerAuthoringOffers } from "./offer-handlers";

it("the Plan follow-ups offer starts an authoring chat", () => {
  const send = vi.fn();
  const unregister = registerAuthoringOffers(send);
  try {
    const offer: OfferView = {
      id: "plan-follow-ups",
      kind: "plan_follow_ups",
      label: "Plan follow-ups",
      target: null,
    };
    expect(runOffer(offer)).toBe(true);
    expect(send).toHaveBeenCalledWith({ offer: "plan-follow-ups" });
  } finally {
    unregister();
  }
});
