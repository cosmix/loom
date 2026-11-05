import type { ChecklistView } from "@/plan/api/schema";
import { isBuilt } from "@/plan/feature/built-model";
import { featureHref } from "@/plan/feature/use-feature";

/**
 * Where `/f/:feature` lands: a feature whose every plan is done opens on what was built; one with a
 * product spec opens on its product overview; one with only a metaplan opens on the engineering
 * overview; anything else opens on the vision, which offers to start one.
 */
export function landingPath(checklist: ChecklistView): string {
  const { feature } = checklist;
  if (isBuilt(checklist)) return featureHref(feature, "built");
  if (checklist.has_product) return featureHref(feature, "product");
  return featureHref(feature, checklist.metaplans.length > 0 ? "engineering" : "vision");
}
