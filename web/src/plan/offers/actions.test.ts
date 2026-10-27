import { describe, expect, it, vi } from "vitest";
import type { OfferView } from "@/plan/api/schema";
import { offerAction, registerOfferAction, runOffer } from "@/plan/offers/actions";

const offer: OfferView = {
  id: "offer-1",
  kind: "pressure_test",
  label: "Pressure test",
  target: null,
};

describe("offer actions", () => {
  it("runs the registered handler with the offer and unregisters it", () => {
    const handler = vi.fn();
    expect(offerAction(offer.kind)).toBeUndefined();
    expect(runOffer(offer)).toBe(false);

    const unregister = registerOfferAction(offer.kind, handler);
    expect(offerAction(offer.kind)).toBe(handler);
    expect(runOffer(offer)).toBe(true);
    expect(handler).toHaveBeenCalledOnce();
    expect(handler).toHaveBeenCalledWith(offer);

    unregister();
    expect(offerAction(offer.kind)).toBeUndefined();
    expect(runOffer(offer)).toBe(false);
  });

  it("keeps a replacement when an earlier registration is removed", () => {
    const handler = vi.fn();
    const unregisterOld = registerOfferAction(offer.kind, handler);
    const unregisterNew = registerOfferAction(offer.kind, handler);

    unregisterOld();
    expect(runOffer(offer)).toBe(true);
    unregisterNew();
    expect(runOffer(offer)).toBe(false);
  });

  it("keeps registrations for different kinds independent", () => {
    const pressure = vi.fn();
    const start = vi.fn();
    const unregisterPressure = registerOfferAction("pressure_test", pressure);
    const unregisterStart = registerOfferAction("start_run", start);

    unregisterPressure();
    expect(runOffer(offer)).toBe(false);
    const startOffer: OfferView = { ...offer, kind: "start_run" };
    expect(runOffer(startOffer)).toBe(true);
    expect(start).toHaveBeenCalledWith(startOffer);
    unregisterStart();
  });
});
