import { createContext, useContext } from "react";

import type { ConceptPage, Mark, Sentence, SourceAnchor } from "@/plan/api/schema-concept";

import { anchorKey, type CitedLookup } from "./open-state";
import type { FocusTarget, FoldNote, OpenRun } from "./types";

/**
 * Where a rendered sentence sits: the summary it belongs to, the run that holds it, and the
 * marks it shows (its own, or the ones its citing sentence or page supply).
 */
export type SentenceSite = {
  sentence: Sentence;
  summary: string;
  parent: string | null;
  marks: Mark[];
};

export type ColumnContextValue = {
  pageKey: string;
  page: ConceptPage;
  open: OpenRun[];
  notes: FoldNote[];
  flashing: ReadonlySet<string>;
  lookup: CitedLookup;
  /** The page key of a cited summary id, and its loaded page. */
  citedPage: (summary: string) => ConceptPage | null;
  docPaths: ReadonlySet<string>;
  openSentence: (site: SentenceSite) => void;
  foldRun: (run: OpenRun) => void;
  /** Opens a concept page (`product/section_01`). */
  openPage: (page: string) => void;
  /** Opens a file-mode address. */
  openAddress: (address: string) => void;
};

export const ColumnContext = createContext<ColumnContextValue | null>(null);

export function useColumn(): ColumnContextValue {
  const value = useContext(ColumnContext);
  if (!value) throw new Error("useColumn needs a ReadingColumn");
  return value;
}

export function sentenceKey(parent: string | null, id: string): string {
  return `s|${parent ?? ""}|${id}`;
}

export function passageKey(sentenceId: string, anchor: string): string {
  return `p|${sentenceId}|${anchor}`;
}

/** The roving-focus key of a reading-order target; elements carry it as `data-focus`. */
export function targetKey(target: FocusTarget): string {
  return target.kind === "sentence"
    ? sentenceKey(target.runOf, target.id)
    : passageKey(target.runOf ?? "", target.id);
}

/** A DOM id safe in `aria-controls`, unique per run. */
export function runDomId(parent: string | null, id: string): string {
  return `concept-run-${(parent ?? "top") + "-" + id}`.replace(/[^A-Za-z0-9_-]/g, "_");
}

export function firstWords(text: string, count = 5): string {
  return text.split(/\s+/).filter(Boolean).slice(0, count).join(" ");
}

/** The source sections a sentence opens to: its source citations, else what it rests on. */
export function passageAnchors(sentence: Sentence): SourceAnchor[] {
  const cited = sentence.citations.flatMap((citation) =>
    citation.kind === "source" ? [{ path: citation.path, anchor: citation.anchor }] : [],
  );
  const anchors = cited.length > 0 ? cited : sentence.rests_on;
  const seen = new Set<string>();
  return anchors.filter((anchor) => {
    const key = anchorKey(anchor);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function sentenceCitations(sentence: Sentence) {
  return sentence.citations.flatMap((citation) => (citation.kind === "sentence" ? [citation] : []));
}

/** Whether a sentence is a button: it cites something it can open. */
export function canOpen(sentence: Sentence): boolean {
  return sentence.citations.length > 0;
}

/** The open run of a sentence at its site, if any. */
export function runAt(open: OpenRun[], site: SentenceSite): OpenRun | undefined {
  return open.find(
    (run) => run.sentenceId === site.sentence.id && run.parent === site.parent && run.level < 2,
  );
}

/** The summary id behind a page key: `engineering/<slug>` pages hold the `plans/<slug>` summary. */
export function summaryIdOfKey(pageKey: string): string {
  return pageKey.startsWith("engineering/")
    ? `plans/${pageKey.slice("engineering/".length)}`
    : pageKey;
}
