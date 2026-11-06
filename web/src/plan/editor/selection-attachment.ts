import type { RefObject } from "react";

import { useSelectionAttachment, type AttachmentDraft } from "@/plan/attachments";

import type { EditorHandle } from "./codemirror";
import type { SelectionLines } from "./editor-provenance";

/**
 * Add to chat for a selection inside CodeMirror. The lines come from the editor that owns
 * the selection (`rangeLines`), so a selection in a pane the handle does not own (the
 * on-disk side of a merge) or outside any editor yields no draft. The text comes from the
 * same range (`rangeText`), so it keeps the Markdown markers live preview hides from the DOM.
 */
export function useEditorSelectionAttachment(
  container: RefObject<HTMLElement | null>,
  handle: RefObject<EditorHandle | null>,
  provenance: (
    selection: Selection,
    lines: SelectionLines,
    text?: string | null,
  ) => AttachmentDraft | null,
) {
  return useSelectionAttachment(container, (selection) => {
    const range = selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
    // A chunk without `rangeLines` (older handle) cannot attribute lines.
    const lines = range ? (handle.current?.rangeLines?.(range) ?? null) : null;
    const text = range ? (handle.current?.rangeText?.(range) ?? null) : null;
    return lines ? provenance(selection, lines, text) : null;
  });
}
