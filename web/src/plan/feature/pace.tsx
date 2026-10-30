import { Footprints, Users, type LucideIcon } from "lucide-react";

import type { ChecklistView, OfferView, StepView } from "@/plan/api/schema";
import { plural } from "@/plan/feature/steps";
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
  icon: Icon,
  title,
  label,
  description,
  offer,
  cost,
}: {
  icon: LucideIcon;
  title: string;
  label?: string;
  description: string;
  offer: OfferView;
  cost?: string;
}) {
  return (
    <section className="plan-pace__card">
      <h3 aria-label={label} className="plan-pace__title">
        <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        {title}
      </h3>
      <p className="plan-pace__copy">{description}</p>
      {cost && <p className="plan-pace__cost">{cost}</p>}
      <div className="plan-pace__offer">
        <OfferButton offer={offer} />
      </div>
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
  return (
    <div role="group" aria-label="Choose a pace" className="plan-pace">
      <PaceOption
        icon={Footprints}
        title={`Guided, one ${document} at a time`}
        description={`A session drafts one ${document} with you, stops, and offers the next once you've read it. Best when the vision is still moving.`}
        offer={{ ...guided, label: `Start guided writing: ${guided.label}` }}
      />
      <PaceOption
        icon={Users}
        title="Draft all with a team"
        label="Draft all, with a team"
        description={`A team of up to five drafts every missing ${document} at once. Teammates agree terms and boundaries as they write; the lead checks the set for consistency, then hands you a review queue.`}
        cost={`Up to ${plural(Math.min(5, missing), "teammate")}, ${plural(missing, "document")}, one run.`}
        offer={team}
      />
    </div>
  );
}
