import { atom, useAtomValue, useStore } from "jotai";
import { useEffect } from "react";

import { isConceptPageKey, summaryKey, VISION_DOC } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import {
  conceptPageAtom,
  loadConceptPage,
  useConceptSubscription,
  type ConceptPageState,
} from "@/plan/concept/page-state";
import { planSocketAtom } from "@/plan/state/atoms";
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

/** The key of the summary page the Summary view opens: a plan's own page, else `docSummaryKey`. */
export function summaryPageKey(doc: DocView): string | null {
  if (doc.kind !== "plan") return docSummaryKey(doc);
  return doc.plan === null ? null : summaryKey(doc.feature, "engineering", doc.plan.slug);
}

/** Whether a summary page holds a summary: the server marks a page whose file is absent `missing`. */
export function hasSummary(page: ConceptPage | null): boolean {
  return page !== null && page.state !== null && page.state.state !== "missing";
}

/**
 * The summary page for `key`, loaded and kept current: the subscription refetches a loaded page
 * on each `concept_changed`, so a build that finishes while the document is open reaches the
 * page atom. A key the server answered 404 for holds no page for the subscription to refetch, so
 * a delta naming it, or a resync, loads it here.
 */
export function useSummaryPage(key: string | null): ConceptPage | null {
  const store = useStore();
  const socket = useAtomValue(planSocketAtom);
  useConceptSubscription();
  useEffect(() => {
    if (key !== null) void loadConceptPage(store, key);
  }, [store, key]);
  useEffect(() => {
    if (key === null || !socket) return;
    const loadIfAbsent = () => {
      if (store.get(conceptPageAtom(key)).page === null) void loadConceptPage(store, key);
    };
    const off = socket.subscribe("concept_changed", (message) => {
      if (message.pages.includes(key)) loadIfAbsent();
    });
    const offResync = socket.onResync(loadIfAbsent);
    return () => {
      off();
      offResync();
    };
  }, [store, socket, key]);
  return useAtomValue(key === null ? NO_PAGE : conceptPageAtom(key)).page;
}

/**
 * The chips of a document. A section or the vision also carries its summary's state and its
 * builders (read from the summary page the "Summary" view opens, `page`) and when its file last
 * changed; the open questions come first for every document kind.
 */
export function useDocChips(doc: DocView, now: number, page: ConceptPage | null): HeadingChip[] {
  const questions = useOpenQuestionChips(doc.path);
  if (docSummaryKey(doc) === null) return questions;
  return [
    ...(page ? summaryChips(page, isRebuilding(page)) : []),
    ...questions,
    ...(page ? builtByChips(page) : []),
    ...(doc.last_change === null ? [] : [changedChip(doc.last_change, now)]),
  ];
}
