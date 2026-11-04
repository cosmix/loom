import type { ChecklistView } from "@/plan/api/schema";
import { featureHref } from "@/plan/feature/use-feature";

/**
 * Where `/f/:feature` lands: a feature with a vision, or with nothing yet, opens on the vision
 * (which offers to start it when missing); one with only a metaplan opens on the plans.
 */
export function landingPath(checklist: ChecklistView): string {
  const { feature } = checklist;
  const metaplanOnly = !checklist.has_product && checklist.metaplans.length > 0;
  return featureHref(feature, metaplanOnly ? "engineering" : "vision");
}
