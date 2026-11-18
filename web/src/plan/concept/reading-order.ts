import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import { siteKey } from "./column-context";
import { runOfSentence, runSentences } from "./fragments";
import type { CitedLookup } from "./open-state";
import type { ColumnTarget, FocusTarget, OpenRun } from "./types";

/**
 * The page's targets in reading order. `page` is the merged view (`columnPage`): a fragment is
 * one sentence here. An open sentence that cites sentences lists what `runSentences` shows for
 * it; an entry whose page has not loaded still lists its citation.
 */
export function readingOrder(
  page: ConceptPage,
  open: OpenRun[],
  passages: Record<string, string[]>,
  lookup: CitedLookup,
): FocusTarget[] {
  const targets: FocusTarget[] = [];
  // Open runs are keyed by sentence, parent and level (`runOfSentence`): a sentence cited by two
  // overview sentences is open only under the one it was opened from.
  const addPassages = (sentence: Sentence, parent: string | null, level: 1 | 2) => {
    if (!runOfSentence(open, sentence, parent)) return;
    for (const key of passages[siteKey(parent, sentence.id)] ?? []) {
      targets.push({ kind: "passage", id: key, level, runOf: sentence.id });
    }
  };
  const addSentence = (sentence: Sentence) => {
    targets.push({ kind: "sentence", id: sentence.id, level: 0, runOf: null });
    if (!runOfSentence(open, sentence, null)) return;
    const entries = runSentences(sentence, lookup);
    if (entries.length === 0) {
      addPassages(sentence, null, 1);
      return;
    }
    for (const entry of entries) {
      targets.push({ kind: "sentence", id: entry.citation.id, level: 1, runOf: sentence.id });
      if (entry.sentence) addPassages(entry.sentence, sentence.id, 2);
    }
  };
  for (const block of page.blocks) {
    if (block.kind === "paragraph") {
      for (const sentence of block.sentences) addSentence(sentence);
    }
  }
  return targets;
}

/**
 * The reading order with each passage naming its run's parent, for the column's focus keys.
 * `readingOrder` lists a run's passages right after the sentence whose run holds them, so that
 * sentence's `runOf` is the run's parent.
 */
export function columnOrder(
  page: ConceptPage,
  open: OpenRun[],
  passages: Record<string, string[]>,
  lookup: CitedLookup,
): ColumnTarget[] {
  let holder: FocusTarget | undefined;
  return readingOrder(page, open, passages, lookup).map((target) => {
    if (target.kind === "sentence") {
      holder = target;
      return target;
    }
    return { ...target, parent: holder?.runOf ?? null };
  });
}
