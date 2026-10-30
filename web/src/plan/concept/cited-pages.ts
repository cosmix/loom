import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useMemo } from "react";

import type { ConceptPage } from "@/plan/api/schema-concept";

import type { KeyedState } from "./flash";
import { pageSentences, type CitedLookup } from "./open-state";
import {
  conceptPageAtom,
  linkedPageKey,
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

/**
 * Page keys of the summaries any sentence of `page` cites that the page can name. A citation of a
 * document with no link (a plan deleted since the overview was committed) is left out: its id is
 * not a page the server has, and fetching it logs a 404.
 */
function citedKeys(page: ConceptPage): string[] {
  const keys = new Set<string>();
  for (const sentence of pageSentences(page)) {
    for (const citation of sentence.citations) {
      if (citation.kind !== "sentence") continue;
      const key = linkedPageKey(page, citation.summary);
      if (key !== null) keys.add(key);
    }
  }
  return [...keys];
}

/**
 * The summary pages an overview's sentences cite: subscribed so a rebuild of one re-renders
 * the run, and prefetched when the page loads or is replaced so opening a run sends no request.
 */
export function useCitedPages(pageKey: string, page: ConceptPage): CitedPages {
  const store = useStore();
  const ownState = useAtomValue(conceptPageAtom(pageKey));
  const joined = useMemo(() => citedKeys(page).join("\n"), [page]);
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
