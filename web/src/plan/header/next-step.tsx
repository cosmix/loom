import type { ChecklistView } from "@/plan/api/schema";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { OfferButton } from "@/plan/offers/offer-button";
import { DocMark } from "@/plan/ui/doc-mark";
import { STEP_DOC_KIND } from "@/plan/ui/glyphs";

export function NextStepCard({ checklist }: { checklist: ChecklistView | null }) {
  if (checklist === null) return null;

  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  const kind = step ? (STEP_DOC_KIND[step.number - 1] ?? "plan") : "plan";
  const sentence =
    checklist.primary?.label ||
    checklist.items[0]?.text ||
    readyLabel(checklist.current) ||
    "Nothing to do next";

  return (
    <div className="flex min-w-0 max-w-full flex-nowrap items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 font-sans text-[13px] text-card-foreground">
      <DocMark kind={kind} size="sm" />
      <span className="hidden min-w-0 flex-1 truncate sm:block" title={sentence}>
        {sentence}
      </span>
      {checklist.primary && (
        <div className="min-w-0 max-w-full shrink [&_button]:block [&_button]:max-w-full [&_button]:truncate [&_button]:text-[13px] [&_span]:block [&_span]:max-w-full [&_span]:truncate">
          <OfferButton offer={checklist.primary} />
        </div>
      )}
    </div>
  );
}
