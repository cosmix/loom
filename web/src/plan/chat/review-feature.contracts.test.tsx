import { expect, it, vi } from "vitest";
import type { OfferView } from "@/plan/api/schema";
import { runOffer } from "@/plan/offers/actions";
import { registerAuthoringOffers } from "./offer-handlers";

it("the Review the feature offer starts an authoring chat", () => {
  const send = vi.fn();
  const unregister = registerAuthoringOffers(send);
  const offer: OfferView = {
    id: "review-feature",
    kind: "review_feature",
    label: "Review the feature",
    target: null,
  };
  try {
    expect(runOffer(offer)).toBe(true);
    expect(send).toHaveBeenCalledWith({ offer: "review-feature" });
  } finally {
    unregister();
  }
});
