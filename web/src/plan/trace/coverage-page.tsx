import { useEffect, useId, useMemo, useState, type KeyboardEvent } from "react";
import { useLocation } from "react-router";

import type { TraceView } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { hashAnchor } from "@/plan/files/address";
import { PlanPage } from "@/plan/feature/plan-page";

import { SettleMark } from "./coverage-marks";
import {
  buildCoverage,
  filterRows,
  findAnchor,
  TILES,
  type CoverageModel,
  type CoverageSection,
  type Filter,
} from "./coverage-model";
import { HeadingRow } from "./coverage-rows";
import { TraceGate } from "./trace-gate";

import "./maps.css";

// `/f/<feature>/coverage`: the spec grouped by section, one row per heading with the plans and
// stages that cover it. The four tiles count the rows and filter them. A row opens to why the
// heading exists, how it is delivered and what was found about it (`coverage-rows.tsx`). The look
// is `maps.css`; this file writes the structure and the `data-*` attributes it reads.

const STANDFIRST =
  "Which parts of the spec are covered by planned work, and what is uncovered or changed.";

function Tiles(props: {
  model: CoverageModel;
  filter: Filter;
  onChange: (filter: Filter) => void;
}) {
  const idBase = useId();
  return (
    <div className="coverage-tiles" role="group" aria-label="Show headings">
      {TILES.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          className="coverage-tile"
          data-tile={id}
          aria-pressed={props.filter === id}
          aria-label={label}
          aria-describedby={`${idBase}-${id}`}
          onClick={() => props.onChange(id)}
        >
          <span id={`${idBase}-${id}`} className="coverage-tile__count">
            {props.model.tiles[id]}
          </span>
          <span className="coverage-tile__label" aria-hidden="true">
            {label}
          </span>
        </button>
      ))}
    </div>
  );
}

function SectionHead({ section, titleId }: { section: CoverageSection; titleId: string }) {
  return (
    <header className="coverage-section__head">
      {section.number !== null && (
        <span className="coverage-section__number">{section.number}</span>
      )}
      <h2 id={titleId} className="coverage-section__title">
        <Untrusted text={section.title} />
      </h2>
      {section.settle !== null && <SettleMark state={section.settle} />}
      {section.written && section.total > 0 && (
        <span className="coverage-section__count">
          {section.covered} of {section.total} covered
        </span>
      )}
    </header>
  );
}

function SectionRows(props: {
  section: CoverageSection;
  titleId: string;
  feature: string;
  open: Set<string>;
  onToggle: (key: string) => void;
}) {
  const { section, titleId, feature, open, onToggle } = props;
  return (
    <>
      <div className="coverage-cols" aria-hidden="true">
        <span>Heading</span>
        <span>Covered by</span>
        <span>Findings</span>
        <span>State</span>
      </div>
      <ul className="coverage-rows" aria-labelledby={titleId}>
        {section.rows.map((row) => (
          <HeadingRow
            key={row.key}
            row={row}
            section={section}
            feature={feature}
            open={open.has(row.key)}
            onToggle={onToggle}
          />
        ))}
      </ul>
    </>
  );
}

function SectionBlock(props: {
  section: CoverageSection;
  feature: string;
  open: Set<string>;
  onToggle: (key: string) => void;
}) {
  const { section, feature, open, onToggle } = props;
  const titleId = useId();
  return (
    <section
      id={section.id}
      className="coverage-section"
      aria-labelledby={titleId}
      data-written={section.written ? "true" : "false"}
    >
      <SectionHead section={section} titleId={titleId} />
      {section.written ? (
        <SectionRows
          section={section}
          titleId={titleId}
          feature={feature}
          open={open}
          onToggle={onToggle}
        />
      ) : (
        <p className="plan-fine coverage-section__quiet">Not written yet.</p>
      )}
    </section>
  );
}

/** Up and Down move between the row toggles in page order; Enter and Space open a row. */
function moveBetweenRows(event: KeyboardEvent<HTMLElement>): void {
  const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
  const from = event.target as HTMLElement;
  if (step === 0 || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  if (!from.hasAttribute("data-row-toggle")) return;
  const toggles = [...event.currentTarget.querySelectorAll<HTMLElement>("[data-row-toggle]")];
  event.preventDefault();
  toggles[toggles.indexOf(from) + step]?.focus();
}

/** Opens and scrolls to the section or heading the address names, once the page is drawn. */
function useAddressedRow(model: CoverageModel, open: (key: string) => void): void {
  const { hash } = useLocation();
  const drawn = model.sections.length > 0;
  useEffect(() => {
    const anchor = hashAnchor(hash);
    const target = drawn && anchor !== null ? findAnchor(model, anchor) : null;
    if (target === null) return;
    if (target.rowKey !== null) open(target.rowKey);
    document.getElementById(target.elementId)?.scrollIntoView?.();
    // The address is the trigger: a refreshed trace must not scroll the reader back to it.
  }, [hash, drawn]);
}

function CoverageMap({ trace, feature }: { trace: TraceView; feature: string }) {
  const model = useMemo(() => buildCoverage(trace, feature), [trace, feature]);
  const [filter, setFilter] = useState<Filter>("all");
  const [open, setOpen] = useState<Set<string>>(new Set());
  const toggle = (key: string) =>
    setOpen((was) => {
      const next = new Set(was);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  useAddressedRow(model, (key) => setOpen((was) => new Set(was).add(key)));

  if (model.sections.length === 0) {
    return (
      <p className="plan-empty">
        Nothing to map yet. Write the vision and its sections first; they are the left of the map.
      </p>
    );
  }
  const shown = filterRows(model, filter);
  return (
    <div className="coverage">
      <Tiles model={model} filter={filter} onChange={setFilter} />
      {/* The arrow keys act on a focused row toggle only; the handler ignores events from anywhere else. */}
      <div className="coverage-body" onKeyDown={moveBetweenRows}>
        {shown.length === 0 && <p className="plan-empty">No heading matches this filter.</p>}
        {shown.map((section) => (
          <SectionBlock
            key={section.id}
            section={section}
            feature={feature}
            open={open}
            onToggle={toggle}
          />
        ))}
      </div>
    </div>
  );
}

/** `/f/:feature/coverage`: which headings of the spec are covered, by what, and which are not. */
export function CoveragePage() {
  return (
    <TraceGate noun="the coverage">
      {(trace, feature) => (
        <PlanPage feature={feature} label="coverage" title="Coverage" lede={STANDFIRST}>
          <CoverageMap trace={trace} feature={feature} />
        </PlanPage>
      )}
    </TraceGate>
  );
}
