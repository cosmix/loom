import type { ChecklistView } from "@/plan/api/schema";
import { featureHref } from "@/plan/feature/use-feature";

/**
 * Where `/f/:feature` lands: a feature with a vision still to finish opens on it, a feature whose
 * listed sections are all written opens on the product overview, and one with only a metaplan
 * opens on the plans.
 */
export function landingPath(checklist: ChecklistView): string {
  const { feature } = checklist;
  if (!checklist.has_product) {
    return featureHref(feature, checklist.metaplans.length > 0 ? "engineering" : "vision");
  }
  const unwritten = checklist.sections.some((section) => !section.written);
  return featureHref(feature, unwritten ? "vision" : "product");
}
