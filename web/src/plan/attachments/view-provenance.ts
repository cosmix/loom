import type { SectionView } from "@/plan/api/schema-files";

import type { AttachmentDraft } from "./types";

function sectionAnchor(range: Range, sections: SectionView[]): string | null {
  const anchors = new Set(sections.map((section) => section.anchor));
  const root = range.startContainer.getRootNode();
  if (!(root instanceof Document || root instanceof DocumentFragment)) return null;

  const start = range.cloneRange();
  start.collapse(true);
  let anchor: string | null = null;
  for (const element of root.querySelectorAll("[id]")) {
    if (anchors.has(element.id) && start.comparePoint(element, 0) <= 0) {
      anchor = element.id;
    }
  }
  return anchor;
}

function stageId(node: Node): string | null {
  const element = node instanceof Element ? node : node.parentElement;
  const row = element?.closest('[id^="stage:"]');
  return row ? row.id.slice("stage:".length) || null : null;
}

/** Builds provenance for a selection already checked against the reading container. */
export function renderedViewProvenance(path: string, sections: SectionView[]) {
  return (selection: Selection): AttachmentDraft | null => {
    if (selection.rangeCount === 0 || selection.isCollapsed) return null;
    const text = selection.toString();
    if (!text) return null;
    const range = selection.getRangeAt(0);
    return {
      kind: "source",
      path,
      anchor: sectionAnchor(range, sections),
      text,
      lines: null,
      stageId: stageId(range.startContainer),
      hash: null,
    };
  };
}
