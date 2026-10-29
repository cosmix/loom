import { useId } from "react";

import type { ChecklistView as Checklist, ItemView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { OfferButton } from "@/plan/offers/offer-button";

export function Item({ item }: { item: ItemView }) {
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-1.5 text-[13px]">
      <span className="text-muted-foreground">Step {item.step}</span>
      <span className="font-medium" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
        <SentenceText text={visibleControls(item.text)} />
      </span>
      {item.path && (
        <code className="font-mono text-xs text-muted-foreground">
          <Untrusted text={item.path} />
        </code>
      )}
      {item.offer && (
        <div className="ml-auto">
          <OfferButton offer={item.offer} />
        </div>
      )}
    </li>
  );
}

export function ChecklistGaps({ checklist }: { checklist: Checklist }) {
  const id = useId();
  return (
    <section aria-label="Gaps" className="space-y-6">
      {checklist.steps.map((step) => {
        const items = checklist.items.filter((item) => item.step === step.number);
        if (items.length === 0) return null;
        const label = `Step ${step.number} · ${step.name}`;
        return (
          <section key={step.number} aria-labelledby={`${id}-step-${step.number}`}>
            <h2 id={`${id}-step-${step.number}`} className="font-display text-lg">
              <Untrusted text={label} />
            </h2>
            <ul className="divide-y divide-hairline">
              {items.map((item, index) => (
                <Item key={`${item.step}:${index}:${item.text}`} item={item} />
              ))}
            </ul>
          </section>
        );
      })}
    </section>
  );
}

export function NextOffers({ checklist }: { checklist: Checklist }) {
  if (!checklist.primary && checklist.secondary.length === 0) return null;
  return (
    <section aria-label="Next" className="flex flex-wrap items-start gap-3">
      {checklist.primary && <OfferButton offer={checklist.primary} />}
      {checklist.secondary.map((offer) => (
        <OfferButton key={offer.id} offer={offer} />
      ))}
    </section>
  );
}
