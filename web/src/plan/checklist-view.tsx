import { useAtomValue } from "jotai/react";

import type { ChecklistView as Checklist, ItemView } from "@/plan/api/schema";
import { StepList, isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { OfferButton } from "@/plan/offers/offer-button";
import { checklistAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

function heading(checklist: Checklist): string {
  const ready = readyLabel(checklist.current);
  if (ready) return ready;
  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  return step ? `Step ${step.number} of ${checklist.steps.length}: ${step.name}` : "Planning";
}

function Item({ item }: { item: ItemView }) {
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-1.5 text-[13px]">
      <span className="text-muted-foreground">Step {item.step}</span>
      <span className="font-medium">{item.text}</span>
      {item.path && <code className="font-mono text-xs text-muted-foreground">{item.path}</code>}
      {item.offer && (
        <span className="ml-auto">
          <OfferButton offer={item.offer} />
        </span>
      )}
    </li>
  );
}

/** The app's first screen: where the plan stands, what to do next, and the gaps. */
export function ChecklistView() {
  const checklist = useAtomValue(checklistAtom);
  if (checklist === null) {
    return (
      <div className="p-6">
        <LoadState state="loading" title="Loading the checklist" />
      </div>
    );
  }
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-6">
      <h1 className="font-display text-2xl">{heading(checklist)}</h1>
      <StepList checklist={checklist} className="flex flex-wrap gap-1" />
      {(checklist.primary || checklist.secondary.length > 0) && (
        <section aria-label="Next" className="flex flex-wrap items-start gap-3">
          {checklist.primary && <OfferButton offer={checklist.primary} />}
          {checklist.secondary.map((offer) => (
            <OfferButton key={offer.id} offer={offer} />
          ))}
        </section>
      )}
      {checklist.items.length > 0 && (
        <section aria-label="Gaps">
          <ul className="divide-y divide-hairline">
            {checklist.items.map((item, index) => (
              <Item key={`${item.step}:${index}:${item.text}`} item={item} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
