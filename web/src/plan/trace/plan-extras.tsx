import { useId, type JSX } from "react";
import { Link } from "react-router";

import type { TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { featureSourceHref } from "@/plan/feature/source-href";
import { pressureChip, pressureIcon } from "@/plan/files/heading-parts";
import { plural } from "@/plan/ui/plural";

import { BriefsCard } from "./briefs-card";
import { implementedHeadings, pressureRecordText, settleWords } from "./doc-model";
import { shownSettle } from "./model";
import { SettleGlyph } from "./settle-glyph";
import { useTrace } from "./state";

import "./trace.css";

/** The last pressure test: its date (as the heading chip words it), its record and a link to it. */
function PressureRecord(props: { feature: string; path: string; date: string }): JSX.Element {
  const { feature, path, date } = props;
  const { doc } = useKeptDoc(path);
  const record = doc ? pressureRecordText(doc, date) : null;
  const Icon = pressureIcon(date);
  const titleId = useId();
  return (
    <section className="trace-pressure" aria-labelledby={titleId}>
      <h2 id={titleId} className="trace-plan__heading">
        Pressure record
      </h2>
      <div className="trace-pressure__card">
        <span className="trace-pressure__glyph" aria-hidden="true">
          <Icon />
        </span>
        <div className="trace-pressure__body">
          <p className="trace-pressure__when">{pressureChip(date, false).label}</p>
          {record !== null && (
            <p className="trace-pressure__text">
              <Untrusted text={record} />
            </p>
          )}
          <Link
            className="trace-pressure__link"
            to={featureSourceHref(feature, path, { anchor: "pressure-tests" })}
          >
            Read the record
          </Link>
        </div>
      </div>
    </section>
  );
}

function sectionNumber(section: TraceDoc | undefined): string | null {
  if (!section || section.number === null) return null;
  return String(section.number).padStart(2, "0");
}

/** "3 spec headings in 2 sections". */
function implementsCount(headings: number, sections: number): string {
  return `${plural(headings, "spec heading")} in ${plural(sections, "section")}`;
}

/** The spec headings the plan, or a stage of it, covers: each with its section and settle state. */
function Implements(props: { feature: string; path: string }): JSX.Element | null {
  const { feature, path } = props;
  const { trace } = useTrace(feature);
  const titleId = useId();
  const rows = trace ? implementedHeadings(trace, path) : [];
  if (rows.length === 0) return null;
  const sections = new Set(rows.map(({ heading }) => heading.path)).size;
  return (
    <section className="trace-implements" aria-labelledby={titleId}>
      <h2 id={titleId} className="trace-plan__heading">
        What it implements
      </h2>
      <p className="trace-plan__count">{implementsCount(rows.length, sections)}</p>
      <ul className="trace-implements__rows">
        {rows.map(({ heading, section }) => {
          const state = shownSettle(section);
          return (
            <li key={`${heading.path}#${heading.anchor}`} className="trace-implements__row">
              <span className="trace-implements__number">{sectionNumber(section)}</span>{" "}
              <Link
                className="trace-implements__link"
                to={featureSourceHref(feature, heading.path, { anchor: heading.anchor })}
              >
                <Untrusted text={heading.title} />
                {section && (
                  <span className="trace-implements__section">
                    {" · "}
                    <Untrusted text={section.title} />
                  </span>
                )}
              </Link>
              {section && state !== null && (
                <span className="trace-implements__state" data-settle={state}>
                  <SettleGlyph state={state} />
                  {settleWords(state, section.role).chip}
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/**
 * Under a plan's heading, before its summary: the pressure record (only when the plan has one,
 * from the page's `last_pressure`), the spec headings it implements and its briefs.
 */
export function PlanExtras(props: {
  feature: string;
  path: string;
  pressure: string | null;
}): JSX.Element {
  const { feature, path, pressure } = props;
  return (
    <div className="trace-plan">
      {pressure !== null && <PressureRecord feature={feature} path={path} date={pressure} />}
      <Implements feature={feature} path={path} />
      <BriefsCard feature={feature} path={path} />
    </div>
  );
}
