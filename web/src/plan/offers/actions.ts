import type { OfferKind, OfferView } from "@/plan/api/schema";

/** `seed` is text the user would have typed first, such as an open finding to follow up on. */
export type OfferHandler = (offer: OfferView, seed?: string) => void;

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

export function runOffer(offer: OfferView, seed?: string): boolean {
  const handler = offerAction(offer.kind);
  if (!handler) return false;
  // An unseeded offer calls its handler with the offer alone: no trailing undefined argument.
  if (seed === undefined) handler(offer);
  else handler(offer, seed);
  return true;
}
