import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useState } from "react";

import { pendingAttachmentsAtom, type AttachmentDraft } from "@/plan/attachments";
import { serverInfoAtom } from "@/plan/state/atoms";
import { ATTACHMENT_LIMIT_NOTE, draftAtom, MAX_ATTACHMENTS, mergeAttachments } from "./drafts";

/**
 * Keeps the pending attachments and the draft's attachments the same list, capped at
 * MAX_ATTACHMENTS; returns the note to show when a batch of adds hit the cap. Both producers
 * remain independent; the sync is synchronous so a batch of adds is capped as one.
 */
export function usePendingDraft(): string | null {
  const store = useStore();
  const project = useAtomValue(serverInfoAtom)?.project_key;
  const [note, setNote] = useState<string | null>(null);
  useAtomValue(draftAtom);
  useEffect(() => {
    let updating = false;
    const sync = (attachments: AttachmentDraft[]) => {
      if (updating) return;
      updating = true;
      if (attachments.length > MAX_ATTACHMENTS) setNote(ATTACHMENT_LIMIT_NOTE);
      const kept =
        attachments.length > MAX_ATTACHMENTS ? attachments.slice(0, MAX_ATTACHMENTS) : attachments;
      store.set(pendingAttachmentsAtom, kept);
      if (store.get(draftAtom).attachments !== kept) {
        store.set(draftAtom, (draft) => ({ ...draft, attachments: kept }));
      }
      updating = false;
    };
    const merged = mergeAttachments(
      store.get(draftAtom).attachments,
      store.get(pendingAttachmentsAtom),
    );
    const offPending = store.sub(pendingAttachmentsAtom, () =>
      sync(store.get(pendingAttachmentsAtom)),
    );
    const offDraft = store.sub(draftAtom, () => sync(store.get(draftAtom).attachments));
    sync(merged);
    return () => {
      offPending();
      offDraft();
    };
  }, [store, project]);
  return note;
}
