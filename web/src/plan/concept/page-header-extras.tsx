import { parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { summaryIdOfKey } from "@/plan/concept/column-context";
import { PlanRunActions } from "@/plan/run/plan-run-actions";
import { BuiltMarker } from "@/plan/review/built-marker";
import { OpenQuestionCount } from "@/plan/review/open-questions";
import { RebuildButton } from "@/plan/summaries/rebuild-button";

export function PageHeaderExtras({ page }: { page: ConceptPage }) {
  if (page.path === null) return null;
  const parts = parsePageKey(page.key);
  const isProductSection =
    parts?.kind === "summary" && parts.scope === "product" && /^section_\d+$/.test(parts.doc);
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      {page.plan_state !== null && <PlanRunActions path={page.path} state={page.plan_state} />}
      <OpenQuestionCount path={page.path} />
      {isProductSection && <BuiltMarker section={page.path} />}
      <RebuildButton ids={[summaryIdOfKey(page.key)]} />
    </div>
  );
}
