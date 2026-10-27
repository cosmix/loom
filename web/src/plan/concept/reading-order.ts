import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import type { FocusTarget, OpenRun } from "./types";

export function readingOrder(
  page: ConceptPage,
  open: OpenRun[],
  passages: Record<string, string[]>,
): FocusTarget[] {
  const targets: FocusTarget[] = [];
  const isOpen = (id: string) => open.some((run) => run.sentenceId === id);
  const addPassages = (id: string, level: 1 | 2) => {
    if (!isOpen(id)) return;
    for (const key of passages[id] ?? []) {
      targets.push({ kind: "passage", id: key, level, runOf: id });
    }
  };
  const addSentence = (sentence: Sentence) => {
    targets.push({ kind: "sentence", id: sentence.id, level: 0, runOf: null });
    if (!isOpen(sentence.id)) return;
    const citations = sentence.citations.filter((citation) => citation.kind === "sentence");
    if (citations.length === 0) {
      addPassages(sentence.id, 1);
      return;
    }
    for (const citation of citations) {
      targets.push({ kind: "sentence", id: citation.id, level: 1, runOf: sentence.id });
      addPassages(citation.id, 2);
    }
  };
  for (const block of page.blocks) {
    if (block.kind === "paragraph") {
      for (const sentence of block.sentences) addSentence(sentence);
    }
  }
  return targets;
}
