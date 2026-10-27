import type { OfferKind, OfferView } from "@/plan/api/schema";

export type OfferHandler = (offer: OfferView) => void;

const actions = new Map<OfferKind, { handler: OfferHandler }>();

export function registerOfferAction(kind: OfferKind, handler: OfferHandler): () => void {
  const registration = { handler };
  actions.set(kind, registration);
  return () => {
    if (actions.get(kind) === registration) actions.delete(kind);
  };
}

export function offerAction(kind: OfferKind): OfferHandler | undefined {
  return actions.get(kind)?.handler;
}

export function runOffer(offer: OfferView): boolean {
  const handler = offerAction(offer.kind);
  if (!handler) return false;
  handler(offer);
  return true;
}
