import { useAtomValue, useStore } from "jotai/react";
import { useCallback } from "react";

import { isOverviewKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";

import { canOpen, runAt, summaryIdOfKey, type SentenceSite } from "./column-context";
import { paragraphIds, runFor } from "./column-model";
import { pageSentences } from "./open-state";
import type { OpenRun } from "./types";
import { clearNotesIn, openRunsAtom } from "./view-state";

/*
 * The header's Open all / Fold all control acts on the `page` its heading draws, never on the
 * route: while a navigation loads, the route already names the next page but the heading still
 * shows the previous one, and the label must describe what the reader sees.
 */

/** The page's openable sentences that are not open yet, at their top-level sites. */
function closedSites(page: ConceptPage, open: OpenRun[]): SentenceSite[] {
  const summary = summaryIdOfKey(page.key);
  return pageSentences(page)
    .filter(canOpen)
    .map((sentence) => ({ sentence, summary, parent: null, marks: sentence.marks }))
    .filter((site) => !runAt(open, site));
}

/** Whether the page has a sentence that opens at all; a page without one draws no control. */
export function hasOpenable(page: ConceptPage): boolean {
  return pageSentences(page).some(canOpen);
}

/** Whether every openable sentence of the page is open, so the control offers Fold all. */
export function useAllOpen(page: ConceptPage): boolean {
  const open = useAtomValue(openRunsAtom(page.key));
  return hasOpenable(page) && closedSites(page, open).length === 0;
}

/**
 * Opens every openable sentence of the page through its open runs, as a click on each would
 * (and clears the fold notes of the paragraphs it opens in). Runs already open stay as they are.
 */
export function useOpenAll(page: ConceptPage): () => void {
  const store = useStore();
  return useCallback(() => {
    const runs = openRunsAtom(page.key);
    const open = store.get(runs);
    const sites = closedSites(page, open);
    if (sites.length === 0) return;
    const overview = isOverviewKey(page.key);
    store.set(runs, [...open, ...sites.map((site) => runFor(site, overview))]);
    const paragraphs = new Set(sites.flatMap((site) => paragraphIds(page, site.sentence.id)));
    for (const id of paragraphs) clearNotesIn(store, page.key, id);
  }, [store, page]);
}

/** Folds the whole page: every open run at every level, nested member runs included. */
export function useFoldAll(page: ConceptPage): () => void {
  const store = useStore();
  return useCallback(() => store.set(openRunsAtom(page.key), []), [store, page.key]);
}
