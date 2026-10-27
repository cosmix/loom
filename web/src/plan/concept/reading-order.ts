import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import type { ColumnTarget, FocusTarget, OpenRun } from "./types";

export function readingOrder(
  page: ConceptPage,
  open: OpenRun[],
  passages: Record<string, string[]>,
): FocusTarget[] {
  const targets: FocusTarget[] = [];
  // Open runs are keyed by sentence, parent and level (`runAt`): a sentence cited by two
  // overview sentences is open only under the one it was opened from.
  const isOpen = (id: string, parent: string | null) =>
    open.some((run) => run.sentenceId === id && run.parent === parent && run.level < 2);
  const addPassages = (id: string, parent: string | null, level: 1 | 2) => {
    if (!isOpen(id, parent)) return;
    for (const key of passages[id] ?? []) {
      targets.push({ kind: "passage", id: key, level, runOf: id });
    }
  };
  const addSentence = (sentence: Sentence) => {
    targets.push({ kind: "sentence", id: sentence.id, level: 0, runOf: null });
    if (!isOpen(sentence.id, null)) return;
    const citations = sentence.citations.filter((citation) => citation.kind === "sentence");
    if (citations.length === 0) {
      addPassages(sentence.id, null, 1);
      return;
    }
    for (const citation of citations) {
      targets.push({ kind: "sentence", id: citation.id, level: 1, runOf: sentence.id });
      addPassages(citation.id, sentence.id, 2);
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
): ColumnTarget[] {
  let holder: FocusTarget | undefined;
  return readingOrder(page, open, passages).map((target) => {
    if (target.kind === "sentence") {
      holder = target;
      return target;
    }
    return { ...target, parent: holder?.runOf ?? null };
  });
}
