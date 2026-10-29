import { useAtomValue } from "jotai/react";

import { OfferButton } from "@/plan/offers/offer-button";
import { checklistAtom } from "@/plan/state/atoms";
import { NewChatButton } from "./start-chat";

/**
 * Where a chat can begin: the checklist's primary offer and "New chat". The state line shows it
 * with no chat, the composer shows it in place of the text box once a chat has ended.
 */
export function NewChatOffers({ offer = true }: { offer?: boolean }) {
  const checklist = useAtomValue(checklistAtom);
  return (
    <>
      {offer && checklist?.primary && <OfferButton offer={checklist.primary} />}
      <NewChatButton />
    </>
  );
}
