import type { OfferKind } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";

const authoringKinds = [
  "start_vision",
  "write_section",
  "add_section_to_vision",
  "start_metaplan",
  "write_plan",
  "draft_all_sections",
  "draft_all_plans",
  "plan_follow_ups",
  "fix_vision_table",
] as const satisfies readonly OfferKind[];

/** What an authoring offer asks for; the command layer adds the rest of the wire message. */
export type OfferStart = { offer: string; feature?: string };

export function registerAuthoringOffers(send: (start: OfferStart) => unknown): () => void {
  const unregister = authoringKinds.map((kind) =>
    registerOfferAction(kind, (offer) => {
      send({ offer: offer.id, ...(offer.feature !== undefined ? { feature: offer.feature } : {}) });
    }),
  );
  return () => unregister.forEach((remove) => remove());
}
