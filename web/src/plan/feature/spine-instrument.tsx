import { Fragment, useId, type ReactNode } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import type {
  CoverageReading,
  FindingsReading,
  FindingsSource,
  Instrument,
} from "@/plan/feature/spine-instruments";
import { INSTRUMENT_NAMES } from "@/plan/feature/spine-instruments";
import { INSTRUMENT_GLYPHS } from "@/plan/ui/glyphs";

import "@/plan/feature/spine-instrument.css";

/*
 * An instrument's well at the foot of its group (ui/DESIGN.md, Surfaces): the ledger's icon, its
 * label and figure on one line, a strip of dots under them and an italic footnote. The link holds
 * the label and the figure, so its name reads "Coverage 18 of 22 headings covered" or "Findings
 * 3 open", and covers the whole well through `::after`; the footnote is the link's description.
 * The dots say nothing the figure and the footnote do not, so they are hidden from assistive
 * technology.
 */

/** A dot of the strip: a heading or a finding, in one of the strip's four states. */
type Dot = "covered" | "uncovered" | "open" | "decided";

function Dots({ dots }: { dots: Dot[] }) {
  return (
    <span className="plan-spine__instrument-section">
      {dots.map((dot, index) => (
        <i key={index} className="plan-spine__instrument-dot" data-dot={dot} />
      ))}
    </span>
  );
}

/** One dot per heading, grouped by section: 2px apart inside a section, 7px between sections. */
function CoverageStrip({ reading }: { reading: CoverageReading }) {
  if (reading.total === 0) return null;
  return (
    <span className="plan-spine__instrument-strip" aria-hidden="true">
      {reading.sections.map((section) => (
        <Dots
          key={section.number}
          dots={section.headings.map((covered) => (covered ? "covered" : "uncovered"))}
        />
      ))}
    </span>
  );
}

/** The strip stops at 40 pips; the findings past them are counted in a trailing "+N". */
const MAX_PIPS = 40;

/** One pip per finding: the open ones first, filled, then the decided ones, hollow. */
function FindingsStrip({ reading }: { reading: FindingsReading }) {
  const { open, decided } = reading;
  if (open + decided === 0) return null;
  const shownOpen = Math.min(open, MAX_PIPS);
  const shownDecided = Math.min(decided, MAX_PIPS - shownOpen);
  const hidden = open + decided - shownOpen - shownDecided;
  const pips = (count: number, dot: Dot) => Array.from<Dot>({ length: count }).fill(dot);
  return (
    <span className="plan-spine__instrument-strip" aria-hidden="true">
      {shownOpen > 0 && <Dots dots={pips(shownOpen, "open")} />}
      {shownDecided > 0 && <Dots dots={pips(shownDecided, "decided")} />}
      {hidden > 0 && <span className="plan-spine__instrument-ref">+{hidden}</span>}
    </span>
  );
}

/** A section number or a run's label inside the footnote, set in mono. */
function Ref({ text }: { text: string }) {
  return (
    <span className="plan-spine__instrument-ref">
      <Untrusted text={text} />
    </span>
  );
}

/**
 * The sections with a heading nothing covers, named in a sentence: "03 and 04", "02, 03 and 04".
 * The separators are text nodes beside the spans, not inside one: the description algorithm
 * trims each span's own text, so " and " inside a span would read "03and 04".
 */
function GapNames({ gaps }: { gaps: string[] }) {
  return (
    <>
      {gaps.map((number, index) => (
        <Fragment key={number}>
          {index > 0 && (index === gaps.length - 1 ? " and " : ", ")}
          <Ref text={number} />
        </Fragment>
      ))}
    </>
  );
}

/**
 * Where the gaps are: the sections by number up to three of them, a count of sections past that,
 * and "every section" when none is whole, so a spec with no plan yet does not list itself.
 */
function GapPlaces({ reading }: { reading: CoverageReading }) {
  const { gaps, sections } = reading;
  if (gaps.length === sections.length && sections.length > 1) return <>every section</>;
  if (gaps.length > 3)
    return (
      <>
        {gaps.length} of {sections.length} sections
      </>
    );
  return <GapNames gaps={gaps} />;
}

function CoverageFoot({ reading }: { reading: CoverageReading }) {
  const missing = reading.total - reading.covered;
  if (reading.total === 0) return <>no heading to cover yet</>;
  if (missing === 0) return <>every heading has a plan</>;
  return (
    <>
      {missing} {missing === 1 ? "heading" : "headings"} open in <GapPlaces reading={reading} />
    </>
  );
}

/** Where the findings came from, by kind: the run reviews by name or count, then your own. */
function FindingsFrom({ sources }: { sources: FindingsSource[] }) {
  const runs = sources.flatMap((source) => (source.run === null ? [] : [source.run]));
  const yours = runs.length < sources.length;
  return (
    <>
      from{" "}
      {runs.length === 1 ? (
        <>
          the <Ref text={runs[0]!} /> review
        </>
      ) : runs.length > 1 ? (
        <>{runs.length} run reviews</>
      ) : null}
      {runs.length > 0 && yours && " and "}
      {yours && "your reviews"}
    </>
  );
}

function FindingsFoot({ reading }: { reading: FindingsReading }) {
  const { sources, decided } = reading;
  if (sources.length === 0) return <>no run review has filed any yet</>;
  return (
    <>
      <FindingsFrom sources={sources} />
      {decided > 0 && <> · {decided} decided</>}
    </>
  );
}

/** The figure after the label, or null where the well shows its label alone. */
function figureOf(instrument: Instrument): ReactNode {
  if (instrument.reading === null) return null;
  if (instrument.id === "findings") {
    const { open } = instrument.reading;
    if (open === 0) return <span className="plan-spine__instrument-clear">all clear</span>;
    return (
      <span className="plan-spine__instrument-figure" data-tone="warning">
        {open} <span className="plan-spine__instrument-suffix">open</span>
      </span>
    );
  }
  const { covered, total } = instrument.reading;
  if (total === 0) return null;
  return (
    <span className="plan-spine__instrument-figure">
      {covered} <span className="plan-spine__instrument-suffix">of {total}</span>{" "}
      <span className="sr-only">headings covered</span>
    </span>
  );
}

function stripOf(instrument: Instrument): ReactNode {
  if (instrument.reading === null) return null;
  return instrument.id === "coverage" ? (
    <CoverageStrip reading={instrument.reading} />
  ) : (
    <FindingsStrip reading={instrument.reading} />
  );
}

function footOf(instrument: Instrument): ReactNode {
  if (instrument.reading === null) return null;
  return instrument.id === "coverage" ? (
    <CoverageFoot reading={instrument.reading} />
  ) : (
    <FindingsFoot reading={instrument.reading} />
  );
}

/** The well: its icon, the linked label and figure, the strip and the footnote. */
export function InstrumentWell({
  instrument,
  to,
  current,
}: {
  instrument: Instrument;
  to: string;
  /** The well's page is the one open: `aria-current` on the link, the accent wash on the well. */
  current: boolean;
}) {
  const footId = useId();
  const Icon = INSTRUMENT_GLYPHS[instrument.id];
  const waiting = instrument.id === "findings" && (instrument.reading?.open ?? 0) > 0;
  const figure = figureOf(instrument);
  const foot = footOf(instrument);
  return (
    <li className="plan-spine__instrument" data-instrument={instrument.id}>
      <div className="plan-spine__instrument-top">
        <span
          className="plan-spine__instrument-mark"
          data-tone={waiting ? "warning" : undefined}
          aria-hidden="true"
        >
          <Icon />
        </span>
        <Link
          to={to}
          aria-current={current ? "page" : undefined}
          aria-describedby={foot === null ? undefined : footId}
          className="plan-spine__instrument-link"
        >
          <span className="plan-spine__instrument-label">{INSTRUMENT_NAMES[instrument.id]}</span>
          {/* The space sits outside the spans: the name algorithm trims each span's own text. */}
          {figure !== null && " "}
          {figure}
        </Link>
      </div>
      {stripOf(instrument)}
      {foot !== null && (
        <p id={footId} className="plan-spine__instrument-foot">
          {foot}
        </p>
      )}
    </li>
  );
}
