import { fileName } from "./passages";
import type { FoldNote, SourceAnchorRef } from "./types";

/* The wording of the notes a carry leaves where a run folded (section 05, "What stays open"). */

function titleOf(anchor: SourceAnchorRef): string {
  return anchor.title ?? anchor.anchor;
}

/** "Section Port was removed from section_02.md". */
export function removedSectionNote(anchor: SourceAnchorRef, paragraphOf: string): FoldNote {
  const text = `Section ${titleOf(anchor)} was removed from ${fileName(anchor.path)}`;
  return { paragraphOf, text, path: anchor.path };
}

/** "This summary was rewritten and no longer draws on Port", named for the first anchor. */
export function rewrittenNote(
  anchor: SourceAnchorRef | undefined,
  paragraphOf: string,
  fallbackPath: string,
): FoldNote {
  if (!anchor) return { paragraphOf, text: "This summary was rewritten", path: fallbackPath };
  const text = `This summary was rewritten and no longer draws on ${titleOf(anchor)}`;
  return { paragraphOf, text, path: anchor.path };
}
