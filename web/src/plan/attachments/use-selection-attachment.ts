import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import type { AttachmentDraft } from "./types";

export type SelectionAttachment = {
  draft: AttachmentDraft | null;
  rect: DOMRect | null;
  clear: () => void;
};

type Snapshot = { draft: AttachmentDraft; rect: DOMRect };

function read(
  container: HTMLElement | null,
  provenance: (sel: Selection) => AttachmentDraft | null,
): Snapshot | null {
  const selection = window.getSelection();
  if (!container || !selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (!container.contains(range.startContainer) || !container.contains(range.endContainer)) {
    return null;
  }
  const draft = provenance(selection);
  return draft ? { draft, rect: range.getBoundingClientRect() } : null;
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

  const clear = useCallback(() => {
    window.getSelection()?.removeAllRanges();
    setSnapshot(null);
  }, []);

  return { draft: snapshot?.draft ?? null, rect: snapshot?.rect ?? null, clear };
}
