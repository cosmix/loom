import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";

import {
  passageAnchors,
  siteKey,
  sentenceCitations,
  sentenceKey,
  summaryIdOfKey,
  targetKey,
  type SentenceSite,
} from "./column-context";
import { runOfSentence, runSentences } from "./fragments";
import { anchorKey, pageSentences, type CitedLookup } from "./open-state";
import type { ColumnTarget, OpenRun } from "./types";

const isTop = (run: OpenRun, id: string) => run.sentenceId === id && run.parent === null;

/**
 * The passages each opened sentence shows, by site (`siteKey`), in the form `readingOrder` takes.
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
    if (!runOfSentence(open, sentence, null)) continue;
    const entries = runSentences(sentence, lookup);
    if (entries.length === 0) out[siteKey(null, sentence.id)] = keysOf(sentence);
    for (const { sentence: cited } of entries) {
      if (cited && runOfSentence(open, cited, sentence.id))
        out[siteKey(sentence.id, cited.id)] = keysOf(cited);
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

/**
 * The top-level run a lost top-level run moved to: of the open runs whose sentence is on the
 * next page (its key in `keys`), the one resting on the most sections the lost one did. A run
 * already open at the top level before the rebuild did not move, so it never counts, and a run
 * of a sentence that left the page does not either, so runs from before a carry are never
 * taken for moved ones.
 */
export function movedRun(
  prevOpen: OpenRun[],
  open: OpenRun[],
  lostId: string,
  keys: ReadonlySet<string>,
): OpenRun | undefined {
  const old = prevOpen.find((run) => isTop(run, lostId));
  if (!old) return undefined;
  const sources = new Set(old.restsOn.map(anchorKey));
  const overlap = (run: OpenRun) => run.restsOn.filter((a) => sources.has(anchorKey(a))).length;
  const candidates = open.filter(
    (run) =>
      run.parent === null &&
      run.level < 2 &&
      keys.has(sentenceKey(null, run.sentenceId)) &&
      !prevOpen.some((before) => isTop(before, run.sentenceId)),
  );
  let best: OpenRun | undefined;
  for (const run of candidates) {
    if (overlap(run) > (best ? overlap(best) : 0)) best = run;
  }
  return best;
}

/** The target at `index` and its ancestors in the order, innermost first. */
function ancestry(order: ColumnTarget[], index: number): ColumnTarget[] {
  const chain = [order[index]!];
  for (let at = index - 1; at >= 0 && chain.at(-1)!.runOf !== null; at -= 1) {
    const candidate = order[at]!;
    const holder = chain.at(-1)!.runOf;
    if (candidate.kind === "sentence" && candidate.id === holder) chain.push(candidate);
  }
  return chain;
}

/** A target's key with the sentence `from` replaced by `to` where it names the holding run. */
function rehomedKey(target: ColumnTarget, from: string, to: string): string {
  const swap = <T extends string | null | undefined>(id: T) => (id === from ? to : id);
  return targetKey({ ...target, runOf: swap(target.runOf), parent: swap(target.parent) });
}

type Snapshot = { order: ColumnTarget[]; open: OpenRun[] };

/**
 * Where the roving focus goes when its target left the reading order (Fold all, a rebuild
 * that folds or moves its run, a removed passage): the nearest surviving ancestor sentence,
 * else the same target inside the run its outermost lost ancestor moved to, else that run's
 * sentence, else the target nearest its old position.
 */
export function survivorKey(prev: Snapshot, next: Snapshot, lostKey: string): string | null {
  if (next.order.length === 0) return null;
  const keys = new Set(next.order.map(targetKey));
  const index = prev.order.findIndex((target) => targetKey(target) === lostKey);
  const chain = index < 0 ? [] : ancestry(prev.order, index);
  const survivor = chain.slice(1).find((target) => keys.has(targetKey(target)));
  if (survivor) return targetKey(survivor);
  const top = chain.at(-1);
  const moved = top && movedRun(prev.open, next.open, top.id, keys);
  if (top && moved) {
    const inside = chain
      .slice(0, -1)
      .map((target) => rehomedKey(target, top.id, moved.sentenceId))
      .find((key) => keys.has(key));
    return inside ?? sentenceKey(null, moved.sentenceId);
  }
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
  const entry = citing && runSentences(citing, lookup).find((item) => item.sentence?.id === id);
  return entry?.sentence
    ? { sentence: entry.sentence, summary: entry.citation.summary, parent, marks: [] }
    : null;
}
