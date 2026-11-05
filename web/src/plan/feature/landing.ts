import type { ChecklistView } from "@/plan/api/schema";
import { featureHref } from "@/plan/feature/use-feature";

/**
 * Where `/f/:feature` lands: a feature with a product spec opens on its product overview; one with
 * only a metaplan opens on the engineering overview; anything else opens on the vision, which
 * offers to start one.
 */
export function landingPath(checklist: ChecklistView): string {
  const { feature } = checklist;
  if (checklist.has_product) return featureHref(feature, "product");
  return featureHref(feature, checklist.metaplans.length > 0 ? "engineering" : "vision");
}
