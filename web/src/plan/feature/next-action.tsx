import { useLocation } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { nextSentence } from "@/plan/feature/next-action-copy";
import { pageDrawsPrimary } from "@/plan/feature/page-offer";
import { OfferButton } from "@/plan/offers/offer-button";

/**
 * The next step, at the head of the spine: one sentence and, for the checklist's primary offer,
 * one button. The button is the screen's one primary (DESIGN.md, Buttons). It is left out while
 * the page itself draws that offer (the pace choice, a missing document), so no screen shows an
 * offer twice; a feature with nothing to start shows the sentence only.
 */
export function NextAction({ checklist }: { checklist: ChecklistView }) {
  const { pathname } = useLocation();
  const { primary } = checklist;
  const pageHasIt = pageDrawsPrimary(checklist, pathname);
  return (
    <div className="plan-spine-next">
      <p className="plan-spine-next__text">
        <Untrusted text={nextSentence(checklist)} />
      </p>
      {primary !== null && !pageHasIt && <OfferButton offer={primary} look="primary" />}
    </div>
  );
}
