import { atom, useAtomValue, useStore } from "jotai";
import { useEffect } from "react";

import { summaryKey } from "@/plan/api/page-key";
import type { DocView } from "@/plan/api/schema-files";
import {
  conceptPageAtom,
  loadConceptPage,
  useConceptSubscription,
  type ConceptPageState,
} from "@/plan/concept/page-state";
import type { HeadingChip } from "@/plan/ui/doc-heading";

import {
  builtByChips,
  changedChip,
  isRebuilding,
  summaryChips,
  useOpenQuestionChips,
} from "./heading-parts";

/** What a document without a summary page reads: nothing ever sets it. */
const NO_PAGE = atom<ConceptPageState>({ page: null, previous: null, error: null });

export function stemOf(path: string): string | null {
  return (
    path
      .split("/")
      .pop()
      ?.replace(/\.[^.]+$/, "") || null
  );
}

/** The key of the summary page of a section document; null for any other document. */
export function sectionSummaryKey(doc: DocView): string | null {
  if (doc.kind !== "section" || doc.feature === null) return null;
  const stem = stemOf(doc.path);
  return stem ? summaryKey(doc.feature, "product", stem) : null;
}

/**
 * The chips of a document. A section also carries its summary's state and its builders (read
 * from the summary page the "Summary" view opens) and when its file last changed; the open
 * questions come first for every document kind.
 */
export function useDocChips(doc: DocView, now: number): HeadingChip[] {
  const store = useStore();
  const questions = useOpenQuestionChips(doc.path);
  const key = sectionSummaryKey(doc);
  useConceptSubscription();
  useEffect(() => {
    if (key !== null) void loadConceptPage(store, key);
  }, [store, key]);
  const page = useAtomValue(key === null ? NO_PAGE : conceptPageAtom(key)).page;
  if (key === null) return questions;
  return [
    ...(page ? summaryChips(page, isRebuilding(page)) : []),
    ...questions,
    ...(page ? builtByChips(page) : []),
    ...(doc.last_change === null ? [] : [changedChip(doc.last_change, now)]),
  ];
}
