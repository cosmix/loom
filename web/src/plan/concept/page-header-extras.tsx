import type { ConceptPage } from "@/plan/api/schema-concept";
import { BuiltMarker } from "@/plan/review/built-marker";
import { OpenQuestionCount } from "@/plan/review/open-questions";

export function PageHeaderExtras({ page }: { page: ConceptPage }) {
  if (page.path === null) return null;
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      <OpenQuestionCount path={page.path} />
      {/^product\/section_\d+$/.test(page.key) && <BuiltMarker section={page.path} />}
    </div>
  );
}
