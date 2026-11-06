import { useAtomValue } from "jotai/react";
import type { JSX } from "react";

import { parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { summaryIdOfKey } from "@/plan/concept/column-context";
import { BuiltMarker } from "@/plan/review/built-marker";
import { PlanRunActions } from "@/plan/run/plan-run-actions";
import { ApprovalAnswer, ApprovalSentence } from "@/plan/summaries/approval-notice";
import { RebuildButton } from "@/plan/summaries/rebuild-button";
import { summaryQuestionAtom } from "@/plan/summaries/state";

/**
 * The action at the end of the summary status row: "Rebuild all" while an above-budget rebuild
 * waits for approval, otherwise "Rebuild" for this page's summary; quiet, as every control of
 * the heading is. A page without a source path has nothing to rebuild.
 */
export function SummaryAction({ page }: { page: ConceptPage }): JSX.Element | null {
  const question = useAtomValue(summaryQuestionAtom);
  if (page.path === null) return null;
  if (question !== null) return <ApprovalAnswer look="quiet" />;
  return <RebuildButton ids={[summaryIdOfKey(page.key)]} look="quiet" />;
}

/**
 * The detail line under the summary status: the pending rebuild's one sentence (the count, the
 * reason, the estimate) while an approval is waiting; nothing otherwise.
 */
export function SummaryDetail({ page }: { page: ConceptPage }): JSX.Element | null {
  const question = useAtomValue(summaryQuestionAtom);
  if (page.path === null || question === null) return null;
  return <ApprovalSentence question={question} />;
}

/**
 * What the header's status grid cannot hold: the plan actions and the build comparison that
 * opens a dialog (`.doc-heading-extras`, `ui/doc-heading.css`; the box and the marker's row are
 * hidden by CSS while empty). Open questions, "Built by" and the summary status with its action
 * are chips in `page-heading.tsx`.
 */
export function PageHeaderExtras({ page }: { page: ConceptPage }): JSX.Element | null {
  if (page.path === null) return null;
  const parts = parsePageKey(page.key);
  const isProductSection =
    parts?.kind === "summary" && parts.scope === "product" && /^section_\d+$/.test(parts.doc);
  if (page.plan_state === null && !isProductSection) return null;
  return (
    <div className="doc-heading-extras">
      {page.plan_state !== null && <PlanRunActions path={page.path} state={page.plan_state} />}
      {isProductSection && (
        <div className="doc-heading-extras__marker">
          <BuiltMarker section={page.path} />
        </div>
      )}
    </div>
  );
}
