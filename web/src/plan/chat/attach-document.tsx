import { useAtomValue, useSetAtom } from "jotai";
import { Paperclip } from "lucide-react";

import { addAttachmentAtom } from "@/plan/attachments";
import { DocumentStarter, type StarterDocument } from "@/plan/new/document-starter";
import { draftAtom, MAX_ATTACHMENTS } from "./drafts";

/**
 * The composer's Attach menu: a document in this project, or a file from this computer. Either
 * adds a chip to the draft; an uploaded file arrives already stored and travels by its id.
 */
export function AttachDocument() {
  const add = useSetAtom(addAttachmentAtom);
  const room = Math.max(0, MAX_ATTACHMENTS - useAtomValue(draftAtom).attachments.length);
  const attach = ({ attachment, file }: StarterDocument) => {
    if (file) {
      add({ kind: "file", ...file });
      return;
    }
    if (attachment.kind === "file") return;
    add({
      kind: "source",
      path: attachment.path,
      anchor: null,
      lines: null,
      stageId: null,
      text: attachment.quote,
      hash: attachment.selected_hash,
    });
  };
  return (
    <DocumentStarter onPick={attach} variant="ghost" className="chat-attach" room={room}>
      <Paperclip aria-hidden="true" />
      <span className="sr-only">Attach a file or document</span>
    </DocumentStarter>
  );
}
