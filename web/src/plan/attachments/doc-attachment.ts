import type { DocView } from "@/plan/api/schema-files";
import { quoteLines } from "@/plan/chat/format-attachments";
import type { AttachmentDraft } from "./types";

/** A source attachment for the whole document, the draft that Discuss and the viewed page share. */
export function docAttachment(doc: Pick<DocView, "path" | "text" | "hash">): AttachmentDraft {
  return {
    kind: "source",
    path: doc.path,
    anchor: null,
    lines: null,
    stageId: null,
    // The server passes the quote through (attachments/resolve.rs) and never reads the file. The
    // quote is cut at a line boundary with the marker the formatter writes, so a reader sees the
    // cut and the formatter, which leaves a marked quote alone, cuts nothing twice.
    text: quoteLines(doc.text).join("\n"),
    hash: doc.hash,
  };
}
