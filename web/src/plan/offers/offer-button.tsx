import { useAtomValue } from "jotai/react";

import type { OfferView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { offerAction, runOffer } from "@/plan/offers/actions";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

/**
 * A registered action makes the offer a button that respects the authoring
 * block; with none registered (every offer until plan 9) it reads as text.
 */
export function OfferButton({ offer }: { offer: OfferView }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  if (!offerAction(offer.kind)) return <Untrusted text={offer.label} />;
  return (
    <DisabledReason reason={reason} onClick={() => runOffer(offer)}>
      <Untrusted text={offer.label} />
    </DisabledReason>
  );
}
