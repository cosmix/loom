import type { SectionView } from "@/plan/api/schema-files";
import { editorProvenance, type SelectionLines } from "@/plan/editor/editor-provenance";

/**
 * Builds source provenance for a selection on either side of a diff. Sections describe the
 * document as it is now, so a selection in the before text gets no anchor: its line numbers
 * belong to an older text. (`AttachmentDraft` has no field to say which side the lines are
 * from, so the before side is told apart by the missing anchor only.)
 */
export function diffProvenance(path: string, sections: SectionView[]) {
  const after = editorProvenance(path, sections, []);
  const before = editorProvenance(path, [], []);
  return (selection: Selection, { side, start, end }: SelectionLines) =>
    (side === "before" ? before : after)(selection, { start, end });
}
