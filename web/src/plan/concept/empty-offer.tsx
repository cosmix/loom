import { useAtomValue } from "jotai";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { allOffers } from "@/plan/feature/checklist-offers";
import { OfferButton } from "@/plan/offers/offer-button";
import { canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";

/**
 * The offer as a button where this browser can author (it holds authoring, or nobody does and the
 * server is writable); the offer's label as text for a viewer and in read-only mode. The button
 * runs the registered handler, which asks before replacing a running chat.
 * `checklist` is the one the screen belongs to (the route feature's, else the active one on a
 * standalone page), so the offer comes from the feature the reader is looking at. Only an offer
 * that is that checklist's primary takes the primary look. On a spine screen the one page that
 * reaches that case is a missing vision or metaplan (`MissingDocument`), which owns the offer
 * itself; `EmptyStates` leaves the primary to the spine. A standalone plan page has no spine, so
 * its empty states can draw the active checklist's primary here.
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
  const entry = checklist ? allOffers(checklist).find((candidate) => candidate.id === offer) : null;
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
