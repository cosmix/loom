import { describe, expect, it, vi } from "vitest";
import type { OfferKind, OfferView } from "@/plan/api/schema";
import { offerAction, runOffer } from "@/plan/offers/actions";
import { registerAuthoringOffers } from "./offer-handlers";

const kinds: OfferKind[] = [
  "start_vision",
  "write_section",
  "add_section_to_vision",
  "start_metaplan",
  "write_plan",
  "draft_all_sections",
  "draft_all_plans",
  "plan_follow_ups",
  "fix_vision_table",
];

describe("authoring offer handlers", () => {
  it("registers every authoring kind and unregisters them together", () => {
    const unregister = registerAuthoringOffers(vi.fn());
    try {
      for (const kind of kinds) expect(offerAction(kind)).toBeDefined();
    } finally {
      unregister();
    }
    for (const kind of kinds) expect(offerAction(kind)).toBeUndefined();
  });

  it("sends a start command with each selected offer id", () => {
    const send = vi.fn();
    const unregister = registerAuthoringOffers(send);
    try {
      for (const kind of kinds) {
        const offer: OfferView = { id: `id:${kind}`, kind, label: kind, target: null };
        expect(runOffer(offer)).toBe(true);
        expect(send).toHaveBeenLastCalledWith({ offer: offer.id });
      }
      expect(send).toHaveBeenCalledTimes(kinds.length);
    } finally {
      unregister();
    }
  });

  it("does not register pressure test or start run", () => {
    const unregister = registerAuthoringOffers(vi.fn());
    try {
      expect(offerAction("pressure_test")).toBeUndefined();
      expect(offerAction("start_run")).toBeUndefined();
      expect(offerAction("move_into_feature")).toBeUndefined();
    } finally {
      unregister();
    }
  });

  it("sends the seed as the text of the start", () => {
    const send = vi.fn();
    const unregister = registerAuthoringOffers(send);
    const offer: OfferView = {
      id: "plan-follow-ups",
      kind: "plan_follow_ups",
      label: "Plan follow-ups",
      target: null,
      feature: "demo",
    };
    try {
      runOffer(offer, "Settings look bland");

      expect(send).toHaveBeenCalledExactlyOnceWith({
        offer: "plan-follow-ups",
        feature: "demo",
        text: "Settings look bland",
      });
    } finally {
      unregister();
    }
  });

  it.each(["draft_all_sections", "write_section"] as const)(
    "includes the selected feature for %s",
    (kind) => {
      const send = vi.fn();
      const unregister = registerAuthoringOffers(send);
      const offer: OfferView = {
        id: `id:${kind}`,
        kind,
        label: kind,
        target: null,
        feature: "demo",
      };

      try {
        runOffer(offer);

        expect(send).toHaveBeenCalledExactlyOnceWith({ offer: offer.id, feature: "demo" });
      } finally {
        unregister();
      }
    },
  );
});
