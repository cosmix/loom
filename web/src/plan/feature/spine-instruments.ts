import type { TraceView } from "@/plan/api/schema-trace";
import type { SpineChild } from "@/plan/feature/spine-model";
import { featureHref } from "@/plan/feature/use-feature";
import { buildCoverage } from "@/plan/trace/coverage-model";
import { groupFindings, openCount, shortRun } from "@/plan/trace/findings-model";

// The spine's two instruments: the ledgers read over a group's documents. Coverage closes the
// Product spec group and Findings the Plans group; each well carries its figures from the trace,
// and none until the trace loads, so a well never claims "0 open" before it knows.

export type InstrumentId = "coverage" | "findings";

/** One written section's headings, in order, each true where a plan covers it. */
export interface CoverageSectionDots {
  /** The section's two-digit number, or its file stem for a section without one. */
  number: string;
  headings: boolean[];
}

/**
 * The Coverage well's reading: the Coverage page's own counts over the written sections, one dot
 * per heading grouped by section, and the numbers of the sections with a heading nothing covers.
 */
export interface CoverageReading {
  covered: number;
  total: number;
  sections: CoverageSectionDots[];
  gaps: string[];
}

/**
 * The Findings well's reading: what still waits for a decision, what is decided (planned, folded
 * or dismissed), and the groups the Findings page draws, newest run first.
 */
export interface FindingsReading {
  open: number;
  decided: number;
  sources: FindingsSource[];
}

/** One group of the Findings page: a run review's short label (`03-run`), null for your reviews. */
export interface FindingsSource {
  run: string | null;
}

/** What an instrument well reads. `reading` is null until the trace loads. */
export type Instrument =
  | { id: "coverage"; reading: CoverageReading | null }
  | { id: "findings"; reading: FindingsReading | null };

export const INSTRUMENT_NAMES: Record<InstrumentId, string> = {
  coverage: "Coverage",
  findings: "Findings",
};

/**
 * The Coverage page's own model, so the well and the page agree on every count. A section with
 * no heading yet (a stub, or written without one) draws no group: it has nothing to cover.
 */
function coverageReading(trace: TraceView, feature: string): CoverageReading {
  const withHeadings = buildCoverage(trace, feature).sections.filter(
    (section) => section.rows.length > 0,
  );
  const sections = withHeadings.map((section) => ({
    number: section.number ?? section.id,
    headings: section.rows.map((row) => row.state !== "uncovered"),
  }));
  return {
    covered: withHeadings.reduce((sum, section) => sum + section.covered, 0),
    total: withHeadings.reduce((sum, section) => sum + section.total, 0),
    sections,
    gaps: sections.filter((s) => s.headings.includes(false)).map((s) => s.number),
  };
}

function findingsReading(trace: TraceView, feature: string): FindingsReading {
  const open = openCount(trace);
  return {
    open,
    decided: trace.findings.length - open,
    sources: groupFindings(trace.findings).map((group) => ({
      run: group.run === null ? null : shortRun(group.run, feature),
    })),
  };
}

export function instrumentOf(
  id: InstrumentId,
  feature: string,
  trace: TraceView | null,
): Instrument {
  if (trace === null) return { id, reading: null };
  if (id === "coverage") return { id, reading: coverageReading(trace, feature) };
  return { id, reading: findingsReading(trace, feature) };
}

/** The well of an instrument, the last child of its group: a plain name, linked to the page. */
export function instrumentChild(
  id: InstrumentId,
  feature: string,
  trace: TraceView | null,
): SpineChild {
  return {
    key: id,
    to: featureHref(feature, id),
    name: INSTRUMENT_NAMES[id],
    style: "plain",
    number: null,
    title: null,
    note: null,
    pressure: null,
    glyph: null,
    instrument: instrumentOf(id, feature, trace),
  };
}
