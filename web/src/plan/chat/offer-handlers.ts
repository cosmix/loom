import type { OfferKind } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";

const authoringKinds = [
  "start_vision",
  "write_section",
  "add_section_to_vision",
  "start_metaplan",
  "write_plan",
  "add_plan_to_metaplan",
  "refine_spec",
  "new_metaplan_round",
  "fix_vision_table",
] as const satisfies readonly OfferKind[];

/** What an authoring offer asks for; the command layer adds the rest of the wire message. */
export type OfferStart = { offer: string };

export function registerAuthoringOffers(send: (start: OfferStart) => unknown): () => void {
  const unregister = authoringKinds.map((kind) =>
    registerOfferAction(kind, (offer) => {
      send({ offer: offer.id });
    }),
  );
  return () => unregister.forEach((remove) => remove());
}
