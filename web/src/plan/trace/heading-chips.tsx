import type { HeadingChip } from "@/plan/ui/doc-heading";
import { INSTRUMENT_GLYPHS } from "@/plan/ui/glyphs";

import { sectionCoverage } from "./doc-model";
import { coverageHref, docOfPath } from "./model";
import { useTrace } from "./state";

const CoverageGlyph = INSTRUMENT_GLYPHS.coverage;

/**
 * The chip a section's summary page draws from the trace: "n of m headings covered", linking to
 * Coverage at the section. The document's settle state is not a chip: the settle control's button
 * is the state (`settle-control.tsx`), so the words stand once. Nothing for any other document.
 */
export function useTraceChips(feature: string | null, path: string | null): HeadingChip[] {
  const { trace } = useTrace(feature ?? "");
  const doc = trace && path !== null ? docOfPath(trace, path) : undefined;
  if (!trace || !doc || feature === null || doc.role !== "section") return [];
  const { covered, total } = sectionCoverage(trace, doc.path);
  if (total === 0) return [];
  return [
    {
      key: "coverage",
      label: `${covered} of ${total} headings covered`,
      icon: <CoverageGlyph />,
      to: coverageHref(feature, doc.path),
    },
  ];
}
