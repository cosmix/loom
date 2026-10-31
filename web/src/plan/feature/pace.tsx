import { Footprints, Users, type LucideIcon } from "lucide-react";

import type { ChecklistView, OfferView, StepView } from "@/plan/api/schema";
import { allOffers } from "@/plan/feature/checklist-offers";
import { OfferButton } from "@/plan/offers/offer-button";
import type { ButtonLook } from "@/plan/ui/disabled-reason";
import { plural } from "@/plan/ui/plural";

export function paceOffers(
  checklist: ChecklistView,
  step: StepView,
): { guided: OfferView; team: OfferView } | null {
  const items = checklist.items.filter((item) => item.step === step.number);
  const first = items.find(
    (item) => item.offer?.kind === "write_section" || item.offer?.kind === "write_plan",
  )?.offer;
  if (!first) return null;

  const offers = allOffers(checklist);
  const candidates = offers.filter(
    (offer) => offer.kind === first.kind && items.some((item) => item.offer?.id === offer.id),
  );
  if (first.kind === "write_section") {
    candidates.sort((left, right) => (left.number ?? Infinity) - (right.number ?? Infinity));
  }
  const guided = candidates[0];
  const teamKind = first.kind === "write_section" ? "draft_all_sections" : "draft_all_plans";
  const team = offers.find((offer) => offer.kind === teamKind);
  return guided && team ? { guided, team } : null;
}

function PaceOption({
  icon: Icon,
  title,
  label,
  description,
  offer,
  cost,
  look,
}: {
  icon: LucideIcon;
  title: string;
  label?: string;
  description: string;
  offer: OfferView;
  cost?: string;
  look: ButtonLook;
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
        <OfferButton offer={offer} look={look} />
      </div>
    </section>
  );
}

/** The pace pair and the count of documents still missing, or null when the page has no choice to show. */
export function paceChoice(
  checklist: ChecklistView,
  step: StepView,
): { guided: OfferView; team: OfferView; missing: number } | null {
  const offers = paceOffers(checklist, step);
  if (!offers) return null;
  const { guided, team } = offers;
  const missing = checklist.items.filter(
    (item) => item.step === step.number && item.offer?.kind === guided.kind,
  ).length;
  if (missing === 0 || (guided.feature && guided.feature !== checklist.feature)) return null;
  return { guided, team, missing };
}

export function PaceChoice({ checklist, step }: { checklist: ChecklistView; step: StepView }) {
  const choice = paceChoice(checklist, step);
  if (!choice) return null;
  const { guided, team, missing } = choice;

  const document = guided.kind === "write_section" ? "section" : "plan";
  // Only the card holding the checklist's primary is the screen's primary (DESIGN.md, Buttons).
  const lookOf = (offer: OfferView): ButtonLook =>
    offer.id === checklist.primary?.id ? "primary" : "secondary";
  return (
    <div role="group" aria-label="Choose a pace" className="plan-pace">
      <PaceOption
        icon={Footprints}
        title={`Guided, one ${document} at a time`}
        description={`A session drafts one ${document} with you, stops, and offers the next once you've read it. Best when the vision is still moving.`}
        offer={guided}
        look={lookOf(guided)}
      />
      <PaceOption
        icon={Users}
        title="Draft all with a team"
        label="Draft all, with a team"
        description={`A team of up to five drafts every missing ${document} at once. Teammates agree terms and boundaries as they write; the lead checks the set for consistency, then hands you a review queue.`}
        cost={`Up to ${plural(Math.min(5, missing), "teammate")}, ${plural(missing, "document")}, one run.`}
        offer={team}
        look={lookOf(team)}
      />
    </div>
  );
}
