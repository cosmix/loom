import { isConceptPageKey, pageHref, summaryKey } from "@/plan/api/page-key";
import type { StandalonePlanView } from "@/plan/api/schema";
import { toAddress } from "@/plan/files/address";

/**
 * Where a standalone plan's row opens: its Summary page when the server reports a summary
 * (`has_summary`, the test the plan page's Summary toggle applies through `hasSummary`), else
 * the plan's source. A slug that is not a valid page key has no Summary page and opens the source.
 */
export function standalonePlanHref(
  plan: Pick<StandalonePlanView, "slug" | "path" | "has_summary">,
) {
  const key = summaryKey(null, "engineering", plan.slug);
  return plan.has_summary && isConceptPageKey(key) ? pageHref(key) : toAddress(plan.path);
}
