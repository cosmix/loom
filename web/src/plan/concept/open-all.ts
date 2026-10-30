import { useStore } from "jotai/react";
import { useCallback } from "react";
import { useLocation } from "react-router";

import { isOverviewKey, pageKeyOfPath } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";

import { canOpen, runAt, summaryIdOfKey, type SentenceSite } from "./column-context";
import { paragraphIds, runFor } from "./column-model";
import { pageSentences } from "./open-state";
import { conceptPageAtom } from "./page-state";
import type { OpenRun } from "./types";
import { clearNotesIn, openRunsAtom } from "./view-state";

/** The page's openable sentences that are not open yet, at their top-level sites. */
function closedSites(page: ConceptPage, pageKey: string, open: OpenRun[]): SentenceSite[] {
  const summary = summaryIdOfKey(pageKey);
  return pageSentences(page)
    .filter(canOpen)
    .map((sentence) => ({ sentence, summary, parent: null, marks: sentence.marks }))
    .filter((site) => !runAt(open, site));
}

/**
 * Opens every openable sentence of the concept page the route shows, through its open runs,
 * as a click on each would (and clears the fold notes of the paragraphs it opens in). Runs
 * already open stay as they are. Off a concept route, or before the page has loaded, it does
 * nothing. It reads the page from the route because the summary-page header that offers
 * "Open all" renders beside the reading column, outside its context.
 */
export function useOpenAll(): () => void {
  const store = useStore();
  const { pathname } = useLocation();
  return useCallback(() => {
    const pageKey = pageKeyOfPath(pathname);
    const page = pageKey === null ? null : store.get(conceptPageAtom(pageKey)).page;
    if (pageKey === null || page === null) return;
    const open = store.get(openRunsAtom(pageKey));
    const sites = closedSites(page, pageKey, open);
    if (sites.length === 0) return;
    const overview = isOverviewKey(pageKey);
    store.set(openRunsAtom(pageKey), [...open, ...sites.map((site) => runFor(site, overview))]);
    const paragraphs = new Set(sites.flatMap((site) => paragraphIds(page, site.sentence.id)));
    for (const id of paragraphs) clearNotesIn(store, pageKey, id);
  }, [store, pathname]);
}
