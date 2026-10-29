import type { ConceptPage } from "@/plan/api/schema-concept";
import { PendingApprovalNotice } from "@/plan/summaries/approval-notice";

/** Hosts the above-budget approval notice in the key row; props stay exactly `{ page }`. */
export function KeyRowExtras(_props: { page: ConceptPage }) {
  return <PendingApprovalNotice />;
}
