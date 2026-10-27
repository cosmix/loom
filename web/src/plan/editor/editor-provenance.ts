import type { SectionView } from "@/plan/api/schema-files";
import type { AttachmentDraft } from "@/plan/attachments";

export type StageSpan = { id: string; start_line: number; end_line: number };
export type SelectionLines = { start: number; end: number };

/** Builds a source attachment from a selection and its 1-based inclusive document lines. */
export function editorProvenance(path: string, sections: SectionView[], stages: StageSpan[]) {
  return (selection: Selection, lines: SelectionLines): AttachmentDraft | null => {
    if (selection.rangeCount === 0 || selection.isCollapsed) return null;
    const text = selection.toString();
    if (!text || !Number.isInteger(lines.start) || !Number.isInteger(lines.end)) return null;
    if (lines.start < 1 || lines.end < lines.start) return null;

    const section = sections
      .filter((item) => item.start_line <= lines.start && lines.start <= item.end_line)
      .reduce<SectionView | null>((best, item) => {
        if (!best || item.start_line > best.start_line) return item;
        if (item.start_line === best.start_line && item.level > best.level) return item;
        return best;
      }, null);
    const stage = stages.find(
      (item) => item.start_line <= lines.start && lines.start <= item.end_line,
    );
    return {
      kind: "source",
      path,
      anchor: section?.anchor ?? null,
      text,
      lines: { ...lines },
      stageId: stage?.id ?? null,
      hash: null,
    };
  };
}
