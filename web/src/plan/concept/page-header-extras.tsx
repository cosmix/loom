import { useAtomValue } from "jotai/react";
import type { JSX } from "react";

import { parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { summaryIdOfKey } from "@/plan/concept/column-context";
import { BuiltMarker } from "@/plan/review/built-marker";
import { PlanRunActions } from "@/plan/run/plan-run-actions";
import { PendingApprovalNotice } from "@/plan/summaries/approval-notice";
import { RebuildButton } from "@/plan/summaries/rebuild-button";
import { summaryQuestionAtom } from "@/plan/summaries/state";

/**
 * The action beside the summary status chip, on its line: "Rebuild all" with its one-line
 * explanation while an above-budget rebuild waits for approval, otherwise "Rebuild" for this
 * page's summary. A page without a source path has nothing to rebuild.
 */
export function SummaryAction({ page }: { page: ConceptPage }): JSX.Element | null {
  const question = useAtomValue(summaryQuestionAtom);
  if (page.path === null) return null;
  if (question !== null) return <PendingApprovalNotice />;
  return <RebuildButton ids={[summaryIdOfKey(page.key)]} />;
}

/**
 * What the header's chip row cannot hold: the plan actions and the build comparison that opens a
 * dialog. Open questions, "Built by" and the summary status with its action are chips in
 * `page-heading.tsx`.
 */
export function PageHeaderExtras({ page }: { page: ConceptPage }): JSX.Element | null {
  if (page.path === null) return null;
  const parts = parsePageKey(page.key);
  const isProductSection =
    parts?.kind === "summary" && parts.scope === "product" && /^section_\d+$/.test(parts.doc);
  if (page.plan_state === null && !isProductSection) return null;
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      {page.plan_state !== null && <PlanRunActions path={page.path} state={page.plan_state} />}
      {isProductSection && <BuiltMarker section={page.path} />}
    </div>
  );
}
