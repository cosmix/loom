import type { ChecklistView, OfferView, StepView } from "@/plan/api/schema";
import { OfferButton } from "@/plan/offers/offer-button";

export function paceOffers(
  checklist: ChecklistView,
  step: StepView,
): { guided: OfferView; team: OfferView } | null {
  const items = checklist.items.filter((item) => item.step === step.number);
  const first = items.find(
    (item) => item.offer?.kind === "write_section" || item.offer?.kind === "write_plan",
  )?.offer;
  if (!first) return null;

  const offers = [checklist.primary, ...checklist.secondary, ...items.map((item) => item.offer)];
  const candidates = offers
    .filter((offer) => offer !== null)
    .filter(
      (offer) => offer.kind === first.kind && items.some((item) => item.offer?.id === offer.id),
    );
  if (first.kind === "write_section") {
    candidates.sort((left, right) => (left.number ?? Infinity) - (right.number ?? Infinity));
  }
  const guided = candidates[0];
  const teamKind = first.kind === "write_section" ? "draft_all_sections" : "draft_all_plans";
  const team = offers.find((offer) => offer?.kind === teamKind);
  return guided && team ? { guided, team } : null;
}

function PaceOption({
  title,
  description,
  offer,
  cost,
}: {
  title: string;
  description: string;
  offer: OfferView;
  cost?: string;
}) {
  return (
    <section className="flex min-w-0 flex-col items-start gap-3">
      <h3 className="font-medium text-foreground">{title}</h3>
      <p className="text-sm text-muted-foreground">{description}</p>
      {cost && <p className="text-sm text-muted-foreground">{cost}</p>}
      <OfferButton offer={offer} />
    </section>
  );
}

export function PaceChoice({ checklist, step }: { checklist: ChecklistView; step: StepView }) {
  const offers = paceOffers(checklist, step);
  if (!offers) return null;
  const { guided, team } = offers;
  const missing = checklist.items.filter(
    (item) => item.step === step.number && item.offer?.kind === guided.kind,
  ).length;
  if (missing === 0 || (guided.feature && guided.feature !== checklist.feature)) return null;

  const document = guided.kind === "write_section" ? "section" : "plan";
  const next =
    guided.kind === "write_section" ? "lowest-numbered missing section" : "first missing plan";
  return (
    <div aria-label="Choose a pace" className="grid gap-6 py-4 sm:grid-cols-2">
      <PaceOption
        title="Guided"
        description={`One session drafts the ${next} with you and stops; the next is offered once you have read it.`}
        offer={guided}
      />
      <PaceOption
        title="Draft all, with a team"
        description={`One authoring session leads a team to draft every missing ${document}, then checks the set for consistency and hands you a review queue.`}
        cost={`Up to ${Math.min(5, missing)} teammates, ${missing} documents, one run.`}
        offer={team}
      />
    </div>
  );
}
