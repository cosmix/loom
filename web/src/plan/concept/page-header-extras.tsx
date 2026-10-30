import { parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { summaryIdOfKey } from "@/plan/concept/column-context";
import { BuiltMarker } from "@/plan/review/built-marker";
import { PlanRunActions } from "@/plan/run/plan-run-actions";
import { RebuildButton } from "@/plan/summaries/rebuild-button";

/**
 * What the header's chip row cannot hold: the plan actions, the build comparison that opens a
 * dialog, and the rebuild button. Open questions and "Built by" are chips in `page-heading.tsx`.
 */
export function PageHeaderExtras({ page }: { page: ConceptPage }) {
  if (page.path === null) return null;
  const parts = parsePageKey(page.key);
  const isProductSection =
    parts?.kind === "summary" && parts.scope === "product" && /^section_\d+$/.test(parts.doc);
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      {page.plan_state !== null && <PlanRunActions path={page.path} state={page.plan_state} />}
      {isProductSection && <BuiltMarker section={page.path} />}
      <RebuildButton ids={[summaryIdOfKey(page.key)]} />
    </div>
  );
}
