import { ChevronRight } from "lucide-react";
import { useId } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { SentenceText } from "@/plan/concept/sentence-text";
import { featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";

import { StateMark } from "./coverage-marks";
import { rowElementId, type Chain, type CoverageRow, type CoverageSection } from "./coverage-model";
import { ChangesSinceSettled } from "./settled-changes";
import { summaryHref } from "./model";
import { visionRowText } from "./stub-text";

// The heading rows of the coverage page and what a row opens to. Every string from the documents
// is drawn as text (`Untrusted`), except the titles `Linked` draws with `phrasing`, which
// `SentenceText` reads as Markdown phrasing (code spans, emphasis). The look is `maps.css` and
// `maps-columns.css`.

/** Agent-written text, linked once `to` is known; `phrasing` reads it as Markdown (code spans). */
function Linked(props: { to: string | null; text: string; phrasing?: boolean }) {
  const { to, text, phrasing } = props;
  const label = phrasing ? <SentenceText text={text} /> : <Untrusted text={text} />;
  return to === null ? label : <Link to={to}>{label}</Link>;
}

/** `<plan> › <stage>`, each linked once it has a page. */
function ChainLabel({ chain }: { chain: Chain }) {
  return (
    <span className="coverage-chain">
      <span className="coverage-chain__plan">
        <Linked to={chain.planHref} text={chain.plan} />
      </span>
      {chain.stage !== null && (
        <>
          {" › "}
          <Linked to={chain.stageHref} text={chain.stage} />
        </>
      )}
    </span>
  );
}

function CoveredBy({ chains }: { chains: Chain[] }) {
  return (
    <div className="coverage-cell" data-cell="covered">
      <span className="sr-only">Covered by: </span>
      {chains.length === 0 ? (
        <em className="coverage-nothing">Nothing yet</em>
      ) : (
        <ul className="coverage-chains">
          {chains.map((chain) => (
            <li key={`${chain.planPath ?? chain.entry}|${chain.stage ?? ""}`}>
              <ChainLabel chain={chain} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** The section's vision row, quoted the way the scope line quotes it. */
function VisionQuote({ section, path }: { section: CoverageSection; path: string }) {
  const { doc } = useKeptDoc(path);
  const text =
    doc && section.doc.number !== null ? visionRowText(doc.text, section.doc.number) : null;
  if (text === null) return null;
  return (
    <blockquote className="coverage-quote">
      <strong>
        <Untrusted text={text.title} />
      </strong>
      {text.body !== null && (
        <>
          {" "}
          <Untrusted text={text.body} />
        </>
      )}
    </blockquote>
  );
}

function Why({ section, feature }: { section: CoverageSection; feature: string }) {
  const { stub } = section.doc;
  return (
    <section className="coverage-col" data-col="why">
      <h3>Why</h3> {stub !== null && <VisionQuote section={section} path={stub.path} />}
      <p className="plan-fine coverage-links">
        {stub !== null && (
          <>
            <Link to={featureSourceHref(feature, stub.path, { anchor: stub.anchor })}>
              Vision row
            </Link>
            {" · "}
          </>
        )}
        <Link to={summaryHref(feature, section.doc)}>
          Section {section.number ?? <Untrusted text={section.title} />}
        </Link>
      </p>
    </section>
  );
}

function How({ chains }: { chains: Chain[] }) {
  return (
    <section className="coverage-col" data-col="how">
      <h3>How</h3>{" "}
      {chains.length === 0 && <p className="plan-fine">No entry, plan or stage covers it.</p>}
      {chains.map((chain) => (
        <dl
          className="coverage-how"
          key={`${chain.planPath ?? chain.entry}|${chain.stage ?? ""}`}
          data-changed={chain.changed ? "true" : undefined}
        >
          {chain.entry !== null && (
            <>
              <dt>Entry</dt>
              <dd>
                <Linked to={chain.entryHref} text={chain.entry} phrasing />
              </dd>
            </>
          )}
          <dt>Plan</dt>
          <dd>
            <Linked to={chain.planHref} text={chain.plan} />{" "}
            <span className="plan-fine coverage-plan-state">{chain.planState}</span>
          </dd>
          {chain.stage !== null && (
            <>
              <dt>Stage</dt>
              <dd>
                <Linked to={chain.stageHref} text={chain.stage} />
              </dd>
            </>
          )}
        </dl>
      ))}
    </section>
  );
}

function Findings({ row, feature }: { row: CoverageRow; feature: string }) {
  return (
    <section className="coverage-col" data-col="findings">
      <h3>Findings</h3>{" "}
      {row.open.length === 0 ? (
        <p className="plan-fine">None open.</p>
      ) : (
        <ul>
          {row.open.map((finding) => (
            <li key={finding.id}>
              <Link
                to={`${featureHref(feature, "findings")}#finding-${encodeURIComponent(finding.id)}`}
              >
                <span className="plan-fine coverage-finding-id">
                  <Untrusted text={finding.id} />
                </span>{" "}
                <Untrusted text={finding.title} />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/**
 * Why the plan is flagged: the review module's comparison of the section as it is now against
 * the version the plan was settled at.
 */
function ChangedNote({ chain, section }: { chain: Chain; section: CoverageSection }) {
  if (!chain.changed || chain.planPath === null) return null;
  return (
    <p role="note" className="coverage-note">
      The spec under <Untrusted text={chain.plan} /> changed since you settled it.{" "}
      <ChangesSinceSettled settled={chain.planPath} compare={section.path} look="inline" />
    </p>
  );
}

/** What an expanded row shows: why the heading exists, how it is delivered, what was found. */
function Detail(props: {
  id: string;
  row: CoverageRow;
  section: CoverageSection;
  feature: string;
}) {
  const { id, row, section, feature } = props;
  const flagged = new Map(row.chains.filter((c) => c.changed).map((c) => [c.planPath, c]));
  return (
    <div id={id} className="coverage-detail">
      <div className="coverage-detail__cols">
        <Why section={section} feature={feature} /> <How chains={row.chains} />{" "}
        <Findings row={row} feature={feature} />
      </div>
      {[...flagged.values()].map((chain) => (
        <ChangedNote key={chain.planPath} chain={chain} section={section} />
      ))}
    </div>
  );
}

/**
 * One heading: a toggle named after it, then its cells on the page's shared grid. The toggle is
 * the row's one button, covering the whole line, so Enter opens the row from it; the Covered by
 * cell's plan and stage links follow it in tab order.
 */
export function HeadingRow(props: {
  row: CoverageRow;
  section: CoverageSection;
  feature: string;
  open: boolean;
  onToggle: (key: string) => void;
}) {
  const { row, section, feature, open, onToggle } = props;
  const detailId = useId();
  return (
    <li
      id={rowElementId(section, row)}
      className="coverage-row"
      data-state={row.state}
      data-open={open ? "true" : undefined}
    >
      <div className="coverage-row__line">
        <button
          type="button"
          className="coverage-row__toggle"
          data-row-toggle
          aria-expanded={open}
          aria-controls={open ? detailId : undefined}
          onClick={() => onToggle(row.key)}
        >
          <ChevronRight className="coverage-row__chevron" aria-hidden="true" />
          <Untrusted text={row.title} />
        </button>
        <CoveredBy chains={row.chains} />
        <div className="coverage-cell" data-cell="findings">
          <span className="sr-only">Findings: </span>
          {row.open.length === 0 ? (
            <>
              <span aria-hidden="true">–</span>
              <span className="sr-only">none open</span>
            </>
          ) : (
            `${row.open.length} open`
          )}
        </div>
        <div className="coverage-cell" data-cell="state">
          <StateMark state={row.state} />
        </div>
      </div>
      {open && <Detail id={detailId} row={row} section={section} feature={feature} />}
    </li>
  );
}
