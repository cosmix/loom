import { atom, useAtomValue, useStore } from "jotai";
import { useEffect } from "react";

import { isConceptPageKey, summaryKey, VISION_DOC } from "@/plan/api/page-key";
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

/**
 * The key of the summary page of a feature's vision, one of its sections or its metaplan;
 * null for any other document, or a metaplan file other than `index.md`. The
 * vision's and a metaplan's summary are their own pages, not an overview.
 */
export function docSummaryKey(doc: DocView): string | null {
  if (doc.feature === null) return null;
  if (doc.kind === "vision") return summaryKey(doc.feature, "product", VISION_DOC);
  if (doc.kind !== "section" && doc.kind !== "metaplan") return null;
  return stemSummaryKey(doc.feature, doc.kind === "section" ? "product" : "metaplan", doc.path);
}

function stemSummaryKey(feature: string, scope: "product" | "metaplan", path: string) {
  const stem = stemOf(path);
  if (!stem) return null;
  const key = summaryKey(feature, scope, stem);
  return isConceptPageKey(key) ? key : null;
}

/**
 * The chips of a document. A section or the vision also carries its summary's state and its
 * builders (read from the summary page the "Summary" view opens) and when its file last changed;
 * the open questions come first for every document kind.
 */
export function useDocChips(doc: DocView, now: number): HeadingChip[] {
  const store = useStore();
  const questions = useOpenQuestionChips(doc.path);
  const key = docSummaryKey(doc);
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
