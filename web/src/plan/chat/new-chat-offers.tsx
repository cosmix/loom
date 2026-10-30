import { useAtomValue } from "jotai/react";

import { OfferButton } from "@/plan/offers/offer-button";
import { checklistAtom } from "@/plan/state/atoms";
import { NewChatButton } from "./start-chat";

/** The checklist's primary offer, the empty state's way into a chat. */
export function PrimaryOffer() {
  const checklist = useAtomValue(checklistAtom);
  return checklist?.primary ? <OfferButton offer={checklist.primary} /> : null;
}

/**
 * Where the next chat can begin: the checklist's primary offer and "New chat". The composer shows
 * it in place of the text box once a chat has ended.
 */
export function NewChatOffers({ offer = true }: { offer?: boolean }) {
  return (
    <>
      {offer && <PrimaryOffer />}
      <NewChatButton />
    </>
  );
}
