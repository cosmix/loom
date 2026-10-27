import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import type { AttachmentDraft } from "./types";

export type SelectionAttachment = {
  draft: AttachmentDraft | null;
  rect: DOMRect | null;
  clear: () => void;
};

type Snapshot = { draft: AttachmentDraft; rect: DOMRect };

/** The live selection when it is non-empty and lies inside `container`. */
function selectionIn(container: HTMLElement | null): { selection: Selection; range: Range } | null {
  const selection = window.getSelection();
  if (!container || !selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (!container.contains(range.startContainer) || !container.contains(range.endContainer)) {
    return null;
  }
  return { selection, range };
}

function read(
  container: HTMLElement | null,
  provenance: (sel: Selection) => AttachmentDraft | null,
): Snapshot | null {
  const found = selectionIn(container);
  if (!found) return null;
  const draft = provenance(found.selection);
  return draft ? { draft, rect: found.range.getBoundingClientRect() } : null;
}

function sameRect(a: DOMRect, b: DOMRect): boolean {
  return a.top === b.top && a.left === b.left && a.width === b.width && a.height === b.height;
}

/** The snapshot with its rect re-measured; the same object when nothing moved. */
function remeasure(prev: Snapshot | null, container: HTMLElement | null): Snapshot | null {
  if (!prev) return prev;
  const found = selectionIn(container);
  if (!found) return prev;
  const rect = found.range.getBoundingClientRect();
  return sameRect(prev.rect, rect) ? prev : { draft: prev.draft, rect };
}

/** Tracks the reader's selection inside `container` and turns it into a draft via `provenance`. */
export function useSelectionAttachment(
  container: RefObject<HTMLElement | null>,
  provenance: (sel: Selection) => AttachmentDraft | null,
): SelectionAttachment {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const provenanceRef = useRef(provenance);
  provenanceRef.current = provenance;

  useEffect(() => {
    const update = () => setSnapshot(read(container.current, provenanceRef.current));
    document.addEventListener("selectionchange", update);
    return () => document.removeEventListener("selectionchange", update);
  }, [container]);

  // Scrolling or resizing moves the text under a still-live selection without a selectionchange.
  // Scroll does not bubble, so the capture phase on window catches inner scroll containers too.
  const active = snapshot !== null;
  useEffect(() => {
    if (!active) return;
    const reposition = () => setSnapshot((prev) => remeasure(prev, container.current));
    window.addEventListener("scroll", reposition, { capture: true, passive: true });
    window.addEventListener("resize", reposition);
    return () => {
      window.removeEventListener("scroll", reposition, { capture: true });
      window.removeEventListener("resize", reposition);
    };
  }, [container, active]);

  const clear = useCallback(() => {
    window.getSelection()?.removeAllRanges();
    setSnapshot(null);
  }, []);

  return { draft: snapshot?.draft ?? null, rect: snapshot?.rect ?? null, clear };
}
