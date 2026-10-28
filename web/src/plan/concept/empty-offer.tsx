import { useAtomValue } from "jotai";

import { Untrusted } from "@/plan/chat/visible-controls";
import { OfferButton } from "@/plan/offers/offer-button";
import { canAuthorAtom, checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

/**
 * The offer as a button for the holder, or while nobody holds authoring; text for viewers and in
 * read-only mode. The button runs the registered handler, which asks before replacing a running chat.
 */
export function EmptyOffer({ offer }: { offer: string }) {
  const checklist = useAtomValue(checklistAtom);
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
      <span className="text-[13px] text-muted-foreground">
        <Untrusted text={entry.label} />
      </span>
    );
  }
  return (
    <div className="text-[13px]">
      <OfferButton offer={entry} />
    </div>
  );
}
