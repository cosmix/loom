import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";

import {
  passageAnchors,
  sentenceCitations,
  sentenceKey,
  summaryIdOfKey,
  targetKey,
  type SentenceSite,
} from "./column-context";
import { anchorKey, pageSentences, type CitedLookup } from "./open-state";
import type { FocusTarget, OpenRun } from "./types";

const isTop = (run: OpenRun, id: string) => run.sentenceId === id && run.parent === null;

/**
 * The passages each opened sentence shows, by sentence id, in the form `readingOrder` takes.
 * Only sentences that are open at their own site get an entry, so a cited sentence sharing an
 * id with a page sentence does not borrow that one's passages.
 */
export function passagesRecord(
  page: ConceptPage,
  open: OpenRun[],
  lookup: CitedLookup,
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  const keysOf = (sentence: Sentence) => passageAnchors(sentence).map(anchorKey);
  for (const sentence of pageSentences(page)) {
    if (!open.some((run) => isTop(run, sentence.id) && run.level < 2)) continue;
    const citations = sentenceCitations(sentence);
    if (citations.length === 0) out[sentence.id] = keysOf(sentence);
    for (const citation of citations) {
      const cited = lookup(citation.summary, citation.id);
      const isOpen = open.some(
        (run) => run.level === 1 && run.sentenceId === citation.id && run.parent === sentence.id,
      );
      if (cited && isOpen) out[citation.id] = keysOf(cited);
    }
  }
  return out;
}

/** The run a click or Enter on `site` opens: level 0 over cited sentences, else level 1. */
export function runFor(site: SentenceSite, overview: boolean): OpenRun {
  const { sentence, parent } = site;
  const level = parent !== null ? 1 : sentenceCitations(sentence).length > 0 || overview ? 0 : 1;
  return { level, sentenceId: sentence.id, restsOn: sentence.rests_on, parent };
}

/** The run and every run nested under it, which fold together. */
export function foldedWith(open: OpenRun[], run: OpenRun): OpenRun[] {
  const same = (a: OpenRun, b: OpenRun) =>
    a.sentenceId === b.sentenceId && a.parent === b.parent && a.level === b.level;
  const gone = open.filter((candidate) => same(candidate, run));
  for (let grew = true; grew;) {
    grew = false;
    for (const candidate of open) {
      if (gone.includes(candidate) || candidate.parent === null) continue;
      if (gone.some((parent) => parent.sentenceId === candidate.parent)) {
        gone.push(candidate);
        grew = true;
      }
    }
  }
  return open.filter((candidate) => !gone.includes(candidate));
}

/** Ids of the sentences in the paragraph holding `id`, plus "" for the page's first paragraph. */
export function paragraphIds(page: ConceptPage, id: string): string[] {
  const paragraphs = page.blocks.flatMap((block) => (block.kind === "paragraph" ? [block] : []));
  const at = paragraphs.findIndex((block) => block.sentences.some((s) => s.id === id));
  if (at < 0) return [];
  const ids = paragraphs[at]!.sentences.map((sentence) => sentence.id);
  return at === 0 ? ["", ...ids] : ids;
}

function movedKey(prevOpen: OpenRun[], open: OpenRun[], lostId: string): string | null {
  const old = prevOpen.find((run) => isTop(run, lostId));
  if (!old) return null;
  const sources = new Set(old.restsOn.map(anchorKey));
  const moved = open.find(
    (run) =>
      run.parent === null && run.level < 2 && run.restsOn.some((a) => sources.has(anchorKey(a))),
  );
  return moved ? sentenceKey(null, moved.sentenceId) : null;
}

/**
 * Where the roving focus goes when its target left the reading order (Fold all, a rebuild
 * that folds or moves its run, a removed passage): the nearest surviving ancestor sentence,
 * else the sentence the run moved to, else the target nearest its old position.
 */
export function survivorKey(
  prev: { order: FocusTarget[]; open: OpenRun[] },
  next: { order: FocusTarget[]; open: OpenRun[] },
  lostKey: string,
): string | null {
  if (next.order.length === 0) return null;
  const keys = new Set(next.order.map(targetKey));
  const index = prev.order.findIndex((target) => targetKey(target) === lostKey);
  let current = index < 0 ? undefined : prev.order[index];
  let at = index;
  while (current?.runOf) {
    const ancestorId = current.runOf;
    let found = -1;
    for (let i = at - 1; i >= 0; i -= 1) {
      const candidate = prev.order[i]!;
      if (candidate.kind === "sentence" && candidate.id === ancestorId) {
        found = i;
        break;
      }
    }
    if (found < 0) break;
    const ancestor = prev.order[found]!;
    if (keys.has(targetKey(ancestor))) return targetKey(ancestor);
    current = ancestor;
    at = found;
  }
  const moved = current ? movedKey(prev.open, next.open, current.id) : null;
  if (moved && keys.has(moved)) return moved;
  return targetKey(next.order[Math.min(Math.max(index, 0), next.order.length - 1)]!);
}

/** The sentence a reading-order id names at its site: a page sentence, or one cited by `parent`. */
export function findSite(
  page: ConceptPage,
  lookup: CitedLookup,
  pageKey: string,
  id: string,
  parent: string | null,
): SentenceSite | null {
  const sentences = pageSentences(page);
  if (parent === null) {
    const sentence = sentences.find((item) => item.id === id);
    return sentence
      ? { sentence, summary: summaryIdOfKey(pageKey), parent, marks: sentence.marks }
      : null;
  }
  const citing = sentences.find((item) => item.id === parent);
  const citation = citing && sentenceCitations(citing).find((item) => item.id === id);
  const cited = citation && lookup(citation.summary, id);
  return cited && citation
    ? { sentence: cited, summary: citation.summary, parent, marks: [] }
    : null;
}
