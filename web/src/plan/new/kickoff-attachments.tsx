import { File, FileImage, FileText, X } from "lucide-react";
import { useId } from "react";

import { previewUrl } from "@/plan/attachments/intake";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { docRel } from "@/plan/files/address";
import type { PickedDocument } from "@/plan/new/kickoff-start";
import { formatBytes } from "@/plan/ui/format-bytes";

import "@/plan/chat/chat-chips.css";

/** The row's glyph: an image's own thumbnail, else a file's or a document's mark. */
function AttachmentGlyph({ document }: { document: PickedDocument }) {
  const image = document.file?.media_type.startsWith("image/") ?? false;
  const thumbnail = image && document.file ? previewUrl(document.file.id) : null;
  if (thumbnail !== null) return <img src={thumbnail} alt="" className="chat-chip-thumb" />;
  const Glyph = image ? FileImage : document.file ? File : FileText;
  return (
    <span aria-hidden="true" className="chat-chip-glyph">
      <Glyph />
    </span>
  );
}

/** What stands after the name: an upload's size, or a document's file name. */
function attachmentDetail(document: PickedDocument): string | null {
  if (document.file) return formatBytes(document.file.bytes);
  if (document.attachment.kind === "file") return null;
  return docRel(document.attachment.path).split("/").pop() ?? null;
}

/**
 * One attached document, drawn as the chat composer draws its rows (chat-chips.css): the glyph,
 * the name with its detail after it, and the remove at the row's end. "Attached:" is read, not
 * shown: on screen the row's place under the field says it.
 */
function KickoffAttachment({
  document,
  disabled,
  onRemove,
}: {
  document: PickedDocument;
  disabled: boolean;
  onRemove: () => void;
}) {
  const nameId = useId();
  const label = visibleControls(document.label);
  const detail = attachmentDetail(document);
  return (
    <li className="chat-chip" data-kind={document.file ? "file" : undefined}>
      <AttachmentGlyph document={document} />
      <span id={nameId} className="chat-chip-label" title={label}>
        <span className="sr-only">Attached: </span>
        <Untrusted text={document.label} />
      </span>
      {detail !== null && (
        <span className={document.file ? "chat-chip-size" : "chat-chip-source"}>
          <Untrusted text={detail} />
        </span>
      )}
      {/* The row's name is the button's description; its own name is the bare verb, which the
          screen's tests query. */}
      <button
        type="button"
        className="chat-chip-remove"
        aria-label="Remove"
        aria-describedby={nameId}
        disabled={disabled}
        onClick={onRemove}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </li>
  );
}

/** The attached documents inside the field, under the words; nothing when there are none. */
export function KickoffAttachments({
  documents,
  disabled,
  onRemove,
}: {
  documents: PickedDocument[];
  disabled: boolean;
  onRemove: (id: string) => void;
}) {
  if (!documents.length) return null;
  return (
    <ul aria-label="Attached documents" className="kickoff-attachments chat-chip-list">
      {documents.map((document) => (
        <KickoffAttachment
          key={document.id}
          document={document}
          disabled={disabled}
          onRemove={() => onRemove(document.id)}
        />
      ))}
    </ul>
  );
}
