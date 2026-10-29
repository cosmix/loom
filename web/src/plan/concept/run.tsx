import { useAtomValue } from "jotai";
import { Fragment, type CSSProperties, type JSX } from "react";
import { Link } from "react-router";

import { pageHref } from "@/plan/api/page-key";
import type { Sentence } from "@/plan/api/schema-concept";

import {
  firstWords,
  passageAnchors,
  runAt,
  runDomId,
  sentenceCitations,
  sentenceKey,
  useColumn,
  type SentenceSite,
} from "./column-context";
import { FoldCap } from "./fold-handle";
import { citedMarks, docColour, pageMark } from "./marks";
import { conceptPageAtom, isMissing, pageKeyOfSummary, type ConceptPageState } from "./page-state";
import { PassageBlock } from "./passage";
import { ClosedSentence } from "./sentence";
import type { OpenRun } from "./types";

type NodeProps = { site: SentenceSite; glyph: boolean; flashPage: string; leading?: JSX.Element };

/** A sentence in its current state: closed, or replaced in place by the run it opened to. */
export function SentenceNode({ site, glyph, flashPage, leading }: NodeProps): JSX.Element {
  const column = useColumn();
  const run = runAt(column.open, site);
  if (!run) {
    return (
      <>
        {leading}
        <ClosedSentence site={site} glyph={glyph} flashPage={flashPage} />
      </>
    );
  }
  return sentenceCitations(site.sentence).length > 0 ? (
    <InlineRun site={site} run={run} leading={leading} />
  ) : (
    <PassageRun site={site} run={run} leading={leading} />
  );
}

function groupLabel(site: SentenceSite): string {
  return `Detail of ${firstWords(site.sentence.text)}`;
}

export function runColour(site: SentenceSite, summary: string): string {
  const mark = site.marks.find((mark) => mark.doc.id === summary) ?? site.marks[0];
  return docColour(mark?.colour ?? 0);
}

type RunProps = { site: SentenceSite; run: OpenRun; leading?: JSX.Element };

/**
 * An opened level 0 sentence: the cited level 1 sentences, in citation order, in the same
 * paragraph and the same type, on the faint wash. A cited sentence opened to passages breaks
 * the paragraph inside the run.
 */
function InlineRun({ site, run, leading }: RunProps): JSX.Element {
  const column = useColumn();
  const { sentence, parent } = site;
  const citations = sentenceCitations(sentence);
  const id = runDomId(parent, sentence.id);
  const onFold = () => column.foldRun(run);
  const start = (
    <>
      {leading}
      <FoldCap runId={id} side="start" onFold={onFold} />
    </>
  );
  return (
    <span
      id={id}
      role="group"
      aria-label={groupLabel(site)}
      className="concept-run"
      style={
        { "--run-colour": runColour(site, citations[0]?.summary ?? site.summary) } as CSSProperties
      }
      tabIndex={-1}
      data-focus={sentenceKey(parent, sentence.id)}
    >
      {citations.map((citation, index) => (
        // One sentence may cite the same member sentence twice; the index keeps keys unique.
        <Fragment key={`${index}|${citation.summary}|${citation.id}`}>
          {index > 0 && " "}
          <CitedSentence
            citing={sentence}
            citation={citation}
            first={index === 0 || citations[index - 1]!.summary !== citation.summary}
            leading={index === 0 ? start : undefined}
          />
        </Fragment>
      ))}
      <FoldCap runId={id} side="end" onFold={onFold} />
    </span>
  );
}

type SentenceCitation = ReturnType<typeof sentenceCitations>[number];

type NoticeProps = { text: string; title: string; to: string };

/** A muted notice in a cited sentence's place, linking to the summary page it belongs to. */
function CitedNotice({ text, title, to }: NoticeProps): JSX.Element {
  return (
    <span className="concept-muted">
      {`${text} `}
      <Link to={to} tabIndex={-1} className="underline">
        Read the {title} summary
      </Link>
    </span>
  );
}

/** The notice for a cited summary page whose fetch failed with nothing loaded, else null. */
function failedNotice(state: ConceptPageState, title: string, to: string): JSX.Element | null {
  if (state.page !== null || state.error === null) return null;
  const text = isMissing(state)
    ? `The ${title} summary no longer exists.`
    : `The ${title} summary could not be loaded.`;
  return <CitedNotice text={text} title={title} to={to} />;
}

type CitedProps = {
  citing: Sentence;
  citation: SentenceCitation;
  first: boolean;
  leading?: JSX.Element;
};

/**
 * One cited level 1 sentence inside an opened run. Until its summary page loads it reads
 * "Loading"; when that page failed to load, or no longer holds it (the member summary was
 * rebuilt before the overview), it says so and links to that summary page, so the run stays
 * readable.
 */
function CitedSentence({ citing, citation, first, leading }: CitedProps): JSX.Element {
  const column = useColumn();
  const key = pageKeyOfSummary(column.page, citation.summary);
  const state = useAtomValue(conceptPageAtom(key));
  const page = column.citedPage(citation.summary);
  const cited = column.lookup(citation.summary, citation.id);
  if (!page || !cited) {
    const title = citing.marks.find((mark) => mark.doc.id === citation.summary)?.doc.title;
    const notice = page ? (
      <CitedNotice
        text={`The ${page.title} summary has been rebuilt since this overview.`}
        title={page.title}
        to={pageHref(key)}
      />
    ) : (
      failedNotice(state, title ?? citation.summary, pageHref(key))
    );
    return (
      <>
        {leading}
        {notice ?? <span className="concept-muted">Loading</span>}
      </>
    );
  }
  const fallback = pageMark(page, key, citation.summary);
  return (
    <SentenceNode
      site={{
        sentence: cited,
        summary: citation.summary,
        parent: citing.id,
        marks: citedMarks(citing, citation.summary, cited, fallback),
      }}
      glyph={first}
      flashPage={key}
      leading={leading}
    />
  );
}

/**
 * An opened sentence that rests on source: its passages stack as blocks in citation order and
 * the paragraph resumes after them. The group stands for the sentence in the roving focus.
 */
function PassageRun({ site, run, leading }: RunProps): JSX.Element {
  const column = useColumn();
  const { sentence, parent } = site;
  const anchors = passageAnchors(sentence);
  const id = runDomId(parent, sentence.id);
  const onFold = () => column.foldRun(run);
  return (
    <div
      id={id}
      role="group"
      aria-label={groupLabel(site)}
      className="concept-passages"
      style={{ "--run-colour": runColour(site, site.summary) } as CSSProperties}
      tabIndex={-1}
      data-focus={sentenceKey(parent, sentence.id)}
    >
      <div className="concept-run-edge" data-side="start">
        {/* Keep the enclosing run's cap with this first inline edge, above the block body. */}
        {leading && <span className="concept-run-leading">{leading}</span>}
        <FoldCap runId={id} side="start" onFold={onFold} />
      </div>
      {anchors.length === 0 && <p className="concept-muted">No source sections.</p>}
      {anchors.map((anchor) => (
        <PassageBlock
          key={`${anchor.path}#${anchor.anchor}`}
          parent={parent}
          sentenceId={sentence.id}
          anchor={anchor}
        />
      ))}
      <div className="concept-run-edge" data-side="end">
        <FoldCap runId={id} side="end" onFold={onFold} />
      </div>
    </div>
  );
}
