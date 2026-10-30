import { useAtomValue } from "jotai";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { OfferButton } from "@/plan/offers/offer-button";
import { canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";

/**
 * The offer as a button for the holder, or while nobody holds authoring; text for viewers and in
 * read-only mode. The button runs the registered handler, which asks before replacing a running chat.
 * `checklist` is the one the screen belongs to (the route feature's, else the active one on a
 * standalone page), so the offer comes from the feature the reader is looking at. Only an offer
 * that is that checklist's primary takes the primary look. The one page on a spine screen that
 * reaches that case is a missing vision or metaplan (`MissingDocument`), the only page that owns
 * the offer itself; every other spine screen leaves the primary to the spine.
 */
export function EmptyOffer({
  offer,
  checklist,
}: {
  offer: string;
  checklist: ChecklistView | null;
}) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const server = useAtomValue(serverInfoAtom);
  const entry = [
    checklist?.primary,
    ...(checklist?.secondary ?? []),
    ...(checklist?.items.map((item) => item.offer) ?? []),
  ].find((candidate) => candidate?.id === offer);
  if (!entry) return null;
  if (!canAuthor || (server?.read_only.length ?? 0) > 0) {
    return (
      <span className="plan-fine">
        <Untrusted text={entry.label} />
      </span>
    );
  }
  return (
    <OfferButton
      offer={entry}
      look={entry.id === checklist?.primary?.id ? "primary" : "secondary"}
    />
  );
}
