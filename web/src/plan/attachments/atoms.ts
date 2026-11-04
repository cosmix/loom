import { atom } from "jotai/vanilla";

import type { ChatAttachment } from "./types";

export const pendingAttachmentsAtom = atom<ChatAttachment[]>([]);

function identical(a: ChatAttachment, b: ChatAttachment): boolean {
  if (a.kind === "file" || b.kind === "file") {
    return a.kind === "file" && b.kind === "file" && a.id === b.id;
  }
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

export const addAttachmentAtom = atom(null, (get, set, draft: ChatAttachment) => {
  const pending = get(pendingAttachmentsAtom);
  if (!pending.some((existing) => identical(existing, draft))) {
    set(pendingAttachmentsAtom, [...pending, draft]);
  }
});
