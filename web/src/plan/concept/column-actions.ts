import { useCallback, useRef, type MutableRefObject } from "react";

import { isOverviewKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { Store } from "@/state/store";

import { sentenceKey, type SentenceSite } from "./column-context";
import { foldedWith, paragraphIds, runFor } from "./column-model";
import { runAt } from "./fragments";
import type { OpenRun } from "./types";
import { clearNotesIn, openRunsAtom } from "./view-state";

export type ColumnActions = {
  openSentence: (site: SentenceSite) => void;
  foldRun: (run: OpenRun) => void;
  /** The key of the run just opened, for the open-scroll rule to read once. */
  pendingOpen: MutableRefObject<string | null>;
};

/**
 * Opening and folding write the page's open runs; either one clears the fold notes of the
 * paragraph it happens in (section 05) and moves the roving focus to the sentence concerned.
 */
export function useColumnActions(
  store: Store,
  pageKey: string,
  page: ConceptPage,
  move: (key: string) => void,
): ColumnActions {
  const pendingOpen = useRef<string | null>(null);
  const clearNotes = useCallback(
    (topId: string) => {
      for (const id of paragraphIds(page, topId)) clearNotesIn(store, pageKey, id);
    },
    [store, pageKey, page],
  );
  const openSentence = useCallback(
    (site: SentenceSite) => {
      const current = store.get(openRunsAtom(pageKey));
      const key = sentenceKey(site.parent, site.sentence.id);
      if (!runAt(current, site)) {
        const run = runFor(site, isOverviewKey(pageKey));
        store.set(openRunsAtom(pageKey), [...current, run]);
        clearNotes(site.parent ?? site.sentence.id);
        pendingOpen.current = key;
      }
      move(key);
    },
    [store, pageKey, clearNotes, move],
  );
  const foldRun = useCallback(
    (run: OpenRun) => {
      const current = store.get(openRunsAtom(pageKey));
      store.set(openRunsAtom(pageKey), foldedWith(current, run));
      clearNotes(run.parent ?? run.sentenceId);
      move(sentenceKey(run.parent, run.sentenceId));
    },
    [store, pageKey, clearNotes, move],
  );
  return { openSentence, foldRun, pendingOpen };
}
