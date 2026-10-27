import { Fragment, useRef, type JSX } from "react";

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
import { pageKeyOfSummary } from "./page-state";
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
      {citations.map((citation, index) => {
        const cited = column.lookup(citation.summary, citation.id);
        const first = index === 0 || citations[index - 1]!.summary !== citation.summary;
        const key = pageKeyOfSummary(column.page, citation.summary);
        const page = column.citedPage(citation.summary);
        const fallback = page ? pageMark(page, key, citation.summary) : null;
        return (
          <Fragment key={`${citation.summary}|${citation.id}`}>
            {index > 0 && " "}
            {cited ? (
              <SentenceNode
                site={{
                  sentence: cited,
                  summary: citation.summary,
                  parent: sentence.id,
                  marks: citedMarks(sentence, citation.summary, cited, fallback),
                }}
                glyph={first}
                flashPage={key}
              />
            ) : (
              <span className="concept-muted">Loading</span>
            )}
          </Fragment>
        );
      })}
      <FoldHandle target={ref} depth={parent === null ? 0 : 1} onFold={() => column.foldRun(run)} />
    </span>
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
          sentenceId={sentence.id}
          anchor={anchor}
        />
      ))}
      <FoldHandle target={ref} depth={parent === null ? 0 : 1} onFold={() => column.foldRun(run)} />
    </div>
  );
}
