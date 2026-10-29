import type { ChecklistView, OfferView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { OfferButton } from "@/plan/offers/offer-button";
import { DocMark } from "@/plan/ui/doc-mark";
import { STEP_DOC_KIND } from "@/plan/ui/glyphs";

function offerSentence(offer: OfferView, feature: string): string {
  const owner = offer.feature ?? feature;
  const plan =
    offer.slug ??
    offer.target
      ?.split("/")
      .pop()
      ?.replace(/^PLAN-/, "")
      .replace(/\.md$/, "") ??
    owner;
  const section =
    offer.number === undefined ? (offer.target ?? owner) : String(offer.number).padStart(2, "0");
  switch (offer.kind) {
    case "start_vision":
      return `The product vision for ${owner} is the next document to write.`;
    case "fix_vision_table":
      return `The product vision for ${owner} needs a readable section table.`;
    case "write_section":
      return `Section ${section} is the next product spec document to write.`;
    case "add_section_to_vision":
      return `Section ${section} needs an entry in the product vision's table.`;
    case "start_metaplan":
      return `The product spec for ${owner} is ready for its first metaplan.`;
    case "write_plan":
      return `Plan ${plan} is the next plan to write.`;
    case "pressure_test":
      return `Plan ${plan} is ready for pressure testing.`;
    case "start_run":
      return `The plans for ${owner} are ready to run.`;
    case "refine_spec":
      return `The product spec for ${owner} is ready to reflect what was built.`;
    case "new_metaplan_round":
      return `The next metaplan round for ${owner} is ready to begin.`;
    case "draft_all_sections":
      return `The missing product spec sections for ${owner} are ready to draft.`;
    case "draft_all_plans":
      return `The missing plans for ${owner} are ready to draft.`;
    case "move_into_feature":
      return `Document ${offer.target ?? plan} needs a feature of its own.`;
  }
}

export function NextStepCard({ checklist }: { checklist: ChecklistView | null }) {
  if (checklist === null) return null;

  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  const kind = step ? (STEP_DOC_KIND[step.number - 1] ?? "plan") : "plan";
  const sentence = checklist.primary
    ? offerSentence(checklist.primary, checklist.feature)
    : checklist.items[0]?.text || readyLabel(checklist.current) || "Nothing to do next";

  return (
    <div className="grid min-w-0 max-w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 font-sans text-[13px] text-card-foreground">
      <DocMark kind={kind} size="sm" />
      <span className="min-w-0 break-words whitespace-normal" title={sentence}>
        <Untrusted text={sentence} />
      </span>
      {checklist.primary && (
        <div className="col-span-2 min-w-0 max-w-full justify-self-start sm:col-span-1 sm:col-start-2 [&_button]:block [&_button]:h-auto [&_button]:min-h-8 [&_button]:max-w-full [&_button]:text-[13px] [&_button]:break-words [&_button]:whitespace-normal [&_span]:max-w-full [&_span]:break-words">
          <OfferButton offer={checklist.primary} />
        </div>
      )}
    </div>
  );
}
