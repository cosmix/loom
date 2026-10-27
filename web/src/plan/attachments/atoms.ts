import { atom } from "jotai/vanilla";

import type { AttachmentDraft } from "./types";

export const pendingAttachmentsAtom = atom<AttachmentDraft[]>([]);

function identical(a: AttachmentDraft, b: AttachmentDraft): boolean {
  if (a.kind !== b.kind || a.text !== b.text || a.hash !== b.hash) return false;
  if (a.kind === "source" && b.kind === "source") {
    return (
      a.path === b.path &&
      a.anchor === b.anchor &&
      a.stageId === b.stageId &&
      (a.lines === null
        ? b.lines === null
        : b.lines !== null && a.lines.start === b.lines.start && a.lines.end === b.lines.end)
    );
  }
  if (a.kind === "summary" && b.kind === "summary") {
    return (
      a.summary === b.summary &&
      a.sentence === b.sentence &&
      a.restsOn.length === b.restsOn.length &&
      a.restsOn.every((ref, index) => {
        const other = b.restsOn[index];
        return other != null && ref.path === other.path && ref.anchor === other.anchor;
      })
    );
  }
  return false;
}

export const addAttachmentAtom = atom(null, (get, set, draft: AttachmentDraft) => {
  const pending = get(pendingAttachmentsAtom);
  if (!pending.some((existing) => identical(existing, draft))) {
    set(pendingAttachmentsAtom, [...pending, draft]);
  }
});
