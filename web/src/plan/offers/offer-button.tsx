import { useAtomValue } from "jotai/react";

import type { OfferView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { offerAction, runOffer } from "@/plan/offers/actions";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { type ButtonLook, DisabledReason } from "@/plan/ui/disabled-reason";

/**
 * A registered action makes the offer a button that respects the authoring
 * block; with none registered (every offer until plan 9) it reads as text.
 *
 * An offer is secondary unless its owner says otherwise: only the surface that draws
 * `checklist.primary` passes `look="primary"`, so a screen holds one primary at most. `size="sm"`
 * is the row variant for an offer inside a list row.
 */
export function OfferButton({
  offer,
  look = "secondary",
  size,
}: {
  offer: OfferView;
  look?: ButtonLook;
  size?: "sm";
}) {
  const reason = useAtomValue(authorBlockReasonAtom);
  if (!offerAction(offer.kind)) return <Untrusted text={offer.label} />;
  return (
    <DisabledReason reason={reason} look={look} size={size} onClick={() => runOffer(offer)}>
      <Untrusted text={offer.label} />
    </DisabledReason>
  );
}
