import { useAtomValue } from "jotai/react";
import type { CSSProperties, JSX } from "react";
import { useLocation } from "react-router";

import { docKindOfPageKey, pageKeyOfPath, pageKeyOfSummaryId } from "@/plan/api/page-key";
import { docColour } from "@/plan/concept/marks";
import { conceptPageAtom } from "@/plan/concept/page-state";
import { openDocAtom, treeAtom } from "@/plan/files/atoms";
import { treeDocs } from "@/plan/files/tree-docs";
import { DOC_KIND_COLOUR } from "@/plan/ui/doc-mark";
import { DOC_KIND_GLYPHS } from "@/plan/ui/glyphs";
import { summaryIdOf, type LabelInput } from "./chip-label";
import { usePlanFeature } from "./turn-footer-plans";

/**
 * The glyph at the start of a document chip's row: the document kind's mark in the document's
 * hue, bare, with no tile behind it. The hue says which document, as the key does; it is never a
 * status. A document of no known kind draws nothing, and the row starts at its label.
 */
export function ChipMark({ attachment }: { attachment: LabelInput }): JSX.Element | null {
  const tree = useAtomValue(treeAtom);
  const openDoc = useAtomValue(openDocAtom)?.doc;
  const planFeature = usePlanFeature();
  const location = useLocation();
  const id = attachment.kind === "summary" ? summaryIdOf(attachment.path) : "";
  const key = pageKeyOfSummaryId(id, planFeature);
  const page = useAtomValue(conceptPageAtom(pageKeyOfPath(location.pathname) ?? "")).page;
  const summary = useAtomValue(conceptPageAtom(key ?? "")).page;
  const entry =
    (tree ? treeDocs(tree).find((entry) => entry.path === attachment.path) : null) ??
    (openDoc?.path === attachment.path ? openDoc : null);
  const kind = entry?.kind ?? (key === null ? null : docKindOfPageKey(key));
  const chip = page?.key_chips.find(
    ({ doc }) => doc.path === attachment.path || (key !== null && doc.page === key),
  );
  const colour =
    chip?.colour ?? summary?.colour ?? (page?.path === attachment.path ? page.colour : null);
  if (kind === null || !Object.hasOwn(DOC_KIND_GLYPHS, kind)) return null;
  const docKind = kind as keyof typeof DOC_KIND_GLYPHS;
  const Glyph = DOC_KIND_GLYPHS[docKind];
  const style: CSSProperties = {
    color: colour == null ? `var(--doc-${DOC_KIND_COLOUR[docKind]})` : docColour(colour),
  };
  return (
    <span aria-hidden="true" className="chat-chip-glyph" style={style}>
      <Glyph />
    </span>
  );
}
