import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useMemo } from "react";

import type { ConceptPage } from "@/plan/api/schema-concept";

import type { KeyedState } from "./flash";
import { pageSentences, type CitedLookup } from "./open-state";
import {
  conceptPageAtom,
  loadConceptPages,
  pageKeyOfSummary,
  type ConceptPageState,
} from "./page-state";

export type CitedPages = {
  /** The column's own page state first, then the cited summary pages' states. */
  states: KeyedState[];
  lookup: CitedLookup;
  /** The loaded page behind a cited summary id. */
  citedPage: (summary: string) => ConceptPage | null;
};

/** Summary ids any sentence of `page` cites. */
function citedSummaries(page: ConceptPage): string[] {
  const summaries = new Set<string>();
  for (const sentence of pageSentences(page)) {
    for (const citation of sentence.citations) {
      if (citation.kind === "sentence") summaries.add(citation.summary);
    }
  }
  return [...summaries];
}

/**
 * The summary pages an overview's sentences cite: subscribed so a rebuild of one re-renders
 * the run, and prefetched when the page loads or is replaced so opening a run sends no request.
 */
export function useCitedPages(pageKey: string, page: ConceptPage): CitedPages {
  const store = useStore();
  const ownState = useAtomValue(conceptPageAtom(pageKey));
  const joined = useMemo(
    () => [...new Set(citedSummaries(page).map((id) => pageKeyOfSummary(page, id)))].join("\n"),
    [page],
  );
  const keys = useMemo(() => (joined === "" ? [] : joined.split("\n")), [joined]);
  const statesAtom = useMemo(
    () => atom((get): ConceptPageState[] => keys.map((key) => get(conceptPageAtom(key)))),
    [keys],
  );
  const citedStates = useAtomValue(statesAtom);

  useEffect(() => {
    const missing = keys.filter((_, index) => {
      const state = citedStates[index];
      return state && state.page === null && state.error === null;
    });
    if (missing.length > 0) void loadConceptPages(store, missing);
    // Fetch once per key set; `citedStates` changing as pages arrive must not refetch.
  }, [store, keys]);

  const states = useMemo<KeyedState[]>(
    () => [
      { key: pageKey, state: ownState },
      ...keys.map((key, index) => ({ key, state: citedStates[index]! })),
    ],
    [pageKey, ownState, keys, citedStates],
  );
  return useMemo(() => {
    const byKey = new Map(keys.map((key, index) => [key, citedStates[index]?.page ?? null]));
    const citedPage = (summary: string) => byKey.get(pageKeyOfSummary(page, summary)) ?? null;
    const lookup: CitedLookup = (summary, id) => {
      const cited = citedPage(summary);
      return cited ? pageSentences(cited).find((item) => item.id === id) : undefined;
    };
    return { states, lookup, citedPage };
  }, [states, keys, citedStates, page]);
}
