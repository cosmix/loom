import type { HeadingChip, HeadingTone } from "@/plan/ui/doc-heading";
import { INSTRUMENT_GLYPHS } from "@/plan/ui/glyphs";

import { sectionCoverage, settleWords } from "./doc-model";
import { coverageHref, docOfPath, isMoved, shownSettle } from "./model";
import { SettleGlyph } from "./settle-glyph";
import { useTrace } from "./state";

const CoverageGlyph = INSTRUMENT_GLYPHS.coverage;

/**
 * The chips a summary page's heading draws from the trace: the document's settle state as glyph
 * and words, and for a section "n of m headings covered", linking to Coverage at the section.
 * Nothing for a page with no document in the trace, or for done work, which is never settled.
 */
export function useTraceChips(feature: string | null, path: string | null): HeadingChip[] {
  const { trace } = useTrace(feature ?? "");
  const doc = trace && path !== null ? docOfPath(trace, path) : undefined;
  if (!trace || !doc || feature === null) return [];
  const state = shownSettle(doc);
  const moved = state !== null && isMoved(state);
  const tone: HeadingTone = state === "settled" ? "ok" : moved ? "warning" : "neutral";
  const chips: HeadingChip[] = [];
  if (state !== null) {
    chips.push({
      key: "settle",
      label: settleWords(state, doc.role).chip,
      icon: <SettleGlyph state={state} />,
      tone,
    });
  }
  const { covered, total } = sectionCoverage(trace, doc.path);
  if (doc.role === "section" && total > 0) {
    chips.push({
      key: "coverage",
      label: `${covered} of ${total} headings covered`,
      icon: <CoverageGlyph />,
      to: coverageHref(feature, doc.path),
    });
  }
  return chips;
}
