import { useSetAtom } from "jotai";
import { Paperclip } from "lucide-react";

import { addAttachmentAtom } from "@/plan/attachments";
import { DocumentStarter, type StarterDocument } from "@/plan/new/document-starter";

/** The composer's attach button: the kickoff's document picker, adding a chip to the draft. */
export function AttachDocument() {
  const add = useSetAtom(addAttachmentAtom);
  const attach = ({ attachment }: StarterDocument) =>
    add({
      kind: "source",
      path: attachment.path,
      anchor: null,
      lines: null,
      stageId: null,
      text: attachment.quote,
      hash: attachment.selected_hash,
    });
  return (
    <DocumentStarter onPick={attach} variant="ghost" className="chat-attach">
      <Paperclip aria-hidden="true" />
      <span className="sr-only">Attach a document</span>
    </DocumentStarter>
  );
}
