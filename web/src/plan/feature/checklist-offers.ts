import type { ChecklistView, OfferView } from "@/plan/api/schema";

/** Every offer the checklist carries, in the order the pages look for one: primary, secondary, items. */
export function allOffers(checklist: ChecklistView): OfferView[] {
  return [
    checklist.primary,
    ...checklist.secondary,
    ...checklist.items.map((item) => item.offer),
  ].filter((offer) => offer !== null);
}
