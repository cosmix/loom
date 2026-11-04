import type { Attachment, AttachmentInput } from "@/plan/api/schema-chat";

/**
 * The composer-side wire form of a document chip: the delivered flags (`locked`,
 * `changed_since_selected`) and `file` are the server's to compute, so they are dropped. A file
 * chip has no document form, so a test that passes one is wrong.
 */
export function toWire(chip: Attachment, selected_hash: string | null = null): AttachmentInput {
  const { changed_since_selected: _c, locked: _l, file: _f, kind, ...rest } = chip;
  if (kind === "file") throw new Error("A file chip has no document wire form");
  return { ...rest, kind, selected_hash };
}
