import { useAtomValue } from "jotai/react";

import { OfferButton } from "@/plan/offers/offer-button";
import { checklistAtom } from "@/plan/state/atoms";
import { screenAtom, startOwnerAtom } from "./session-home";
import { NewChatButton } from "./start-chat";

/**
 * The checklist's primary offer, the empty state's way into a chat. The checklist is the last
 * visited feature's, and a click starts a chat owned by the screen it is made on, so the offer
 * shows only on that feature's screens. A mount outside a router has no screen to disagree with.
 */
export function PrimaryOffer() {
  const checklist = useAtomValue(checklistAtom);
  const screen = useAtomValue(screenAtom);
  const owner = useAtomValue(startOwnerAtom);
  if (!checklist?.primary) return null;
  if (screen !== null && owner.feature !== checklist.feature) return null;
  return <OfferButton offer={checklist.primary} />;
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
