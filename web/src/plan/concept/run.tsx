import { useAtomValue } from "jotai";
import { Fragment, useRef, type JSX } from "react";
import { Link } from "react-router";

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
import { FoldHandle } from "./fold-handle";
import { citedMarks, pageMark } from "./marks";
import { conceptPageAtom, isMissing, pageKeyOfSummary, type ConceptPageState } from "./page-state";
import { PassageBlock } from "./passage";
import { ClosedSentence } from "./sentence";
import type { OpenRun } from "./types";

type NodeProps = { site: SentenceSite; glyph: boolean; flashPage: string };

/** A sentence in its current state: closed, or replaced in place by the run it opened to. */
export function SentenceNode({ site, glyph, flashPage }: NodeProps): JSX.Element {
  const column = useColumn();
  const run = runAt(column.open, site);
  if (!run) return <ClosedSentence site={site} glyph={glyph} flashPage={flashPage} />;
  return sentenceCitations(site.sentence).length > 0 ? (
    <InlineRun site={site} run={run} />
  ) : (
    <PassageRun site={site} run={run} />
  );
}

function groupLabel(site: SentenceSite): string {
  return `Detail of ${firstWords(site.sentence.text)}`;
}

/**
 * An opened level 0 sentence: the cited level 1 sentences, in citation order, in the same
 * paragraph and the same type, on the faint wash. A cited sentence opened to passages breaks
 * the paragraph inside the run.
 */
function InlineRun({ site, run }: { site: SentenceSite; run: OpenRun }): JSX.Element {
  const column = useColumn();
  const ref = useRef<HTMLSpanElement>(null);
  const { sentence, parent } = site;
  const citations = sentenceCitations(sentence);
  return (
    <span
      ref={ref}
      id={runDomId(parent, sentence.id)}
      role="group"
      aria-label={groupLabel(site)}
      className="concept-run"
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
          />
        </Fragment>
      ))}
      <FoldHandle target={ref} depth={parent === null ? 0 : 1} onFold={() => column.foldRun(run)} />
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

/**
 * One cited level 1 sentence inside an opened run. Until its summary page loads it reads
 * "Loading"; when that page failed to load, or no longer holds it (the member summary was
 * rebuilt before the overview), it says so and links to that summary page, so the run stays
 * readable.
 */
function CitedSentence({
  citing,
  citation,
  first,
}: {
  citing: Sentence;
  citation: SentenceCitation;
  first: boolean;
}): JSX.Element {
  const column = useColumn();
  const key = pageKeyOfSummary(column.page, citation.summary);
  const state = useAtomValue(conceptPageAtom(key));
  const page = column.citedPage(citation.summary);
  const cited = column.lookup(citation.summary, citation.id);
  if (!page) {
    const title = citing.marks.find((mark) => mark.doc.id === citation.summary)?.doc.title;
    return (
      failedNotice(state, title ?? citation.summary, `/${key}`) ?? (
        <span className="concept-muted">Loading</span>
      )
    );
  }
  if (!cited) {
    const text = `The ${page.title} summary has been rebuilt since this overview.`;
    return <CitedNotice text={text} title={page.title} to={`/${key}`} />;
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
    />
  );
}

/**
 * An opened sentence that rests on source: its passages stack as blocks in citation order and
 * the paragraph resumes after them. The group stands for the sentence in the roving focus.
 */
function PassageRun({ site, run }: { site: SentenceSite; run: OpenRun }): JSX.Element {
  const column = useColumn();
  const ref = useRef<HTMLDivElement>(null);
  const { sentence, parent } = site;
  const anchors = passageAnchors(sentence);
  return (
    <div
      ref={ref}
      id={runDomId(parent, sentence.id)}
      role="group"
      aria-label={groupLabel(site)}
      className="concept-passages"
      tabIndex={-1}
      data-focus={sentenceKey(parent, sentence.id)}
    >
      {anchors.length === 0 && <p className="concept-muted">No source sections.</p>}
      {anchors.map((anchor) => (
        <PassageBlock
          key={`${anchor.path}#${anchor.anchor}`}
          parent={parent}
          sentenceId={sentence.id}
          anchor={anchor}
        />
      ))}
      <FoldHandle target={ref} depth={parent === null ? 0 : 1} onFold={() => column.foldRun(run)} />
    </div>
  );
}
