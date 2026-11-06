import type { ChecklistView, OfferKind, OfferView, StepView } from "@/plan/api/schema";
import { allOffers } from "@/plan/feature/checklist-offers";
import { paceChoice } from "@/plan/feature/pace";
import { STEP } from "@/plan/feature/steps";

export type PlanRow = ChecklistView["plan_rows"][number];

/** The two overview pages that carry gaps above their text, and the step each one's gaps belong to. */
const GAP_STEP = { product: STEP.product, engineering: STEP.plans } as const;
export type GapScope = keyof typeof GAP_STEP;

/** The feature page a pathname shows: `/f/<feature>/<page>[/<doc>]` gives `[page, doc]`. */
export function featurePage(pathname: string): { page: string; doc: string | null } | null {
  const [root, , page, doc] = pathname.split("/").filter(Boolean);
  return root === "f" && page ? { page, doc: doc ?? null } : null;
}

export function gapStep(checklist: ChecklistView, scope: GapScope): StepView | undefined {
  return checklist.steps.find((step) => step.number === GAP_STEP[scope]);
}

/** An offer of this kind wherever the checklist carries one: primary, secondary, then item offers. */
export function offerOfKind(checklist: ChecklistView, kind: OfferKind): OfferView | undefined {
  return allOffers(checklist).find((offer) => offer.kind === kind);
}

/** The offer a missing vision or metaplan page draws for itself. */
export function missingDocumentOffer(
  checklist: ChecklistView,
  page: string,
): OfferView | undefined {
  if (page === "vision" && !checklist.has_product) return offerOfKind(checklist, "start_vision");
  if (page === "metaplan" && checklist.metaplans.length === 0) {
    return offerOfKind(checklist, "start_metaplan");
  }
  return undefined;
}

/** The ids of the offers the pace choice draws on an overview: both cards, or none without a choice. */
export function paceOfferIds(checklist: ChecklistView, scope: GapScope): string[] {
  const step = gapStep(checklist, scope);
  const choice = step === undefined ? null : paceChoice(checklist, step);
  return choice === null ? [] : [choice.guided.id, choice.team.id];
}
