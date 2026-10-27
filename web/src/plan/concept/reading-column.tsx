import { useAtomValue, useStore } from "jotai";
import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent,
} from "react";
import { useNavigate } from "react-router";

import { TooltipProvider } from "@/components/ui/tooltip";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { treeAtom } from "@/plan/files/atoms";
import { docPathsOf } from "@/plan/files/tree";
import type { Store } from "@/state/store";

import { Announcer, type Arrival } from "./announce";
import { useColumnActions } from "./column-actions";
import { ColumnContext, type ColumnContextValue } from "./column-context";
import { useOpenScroll, useRovingFocus, useScrollAnchor } from "./column-focus";
import { handleColumnKey } from "./column-keys";
import { passagesRecord } from "./column-model";
import { useCitedPages } from "./cited-pages";
import { useSentenceFlash } from "./flash";
import { carryOpenState } from "./open-state";
import { citedLookup, conceptPageAtom } from "./page-state";
import { Blocks } from "./paragraph";
import { readingOrder } from "./reading-order";
import { notesAtom, openRunsAtom } from "./view-state";

import "./concept.css";

/** Every document path the page names, so a link in a passage to one of them opens file mode. */
function useDocPaths(page: ConceptPage): ReadonlySet<string> {
  const tree = useAtomValue(treeAtom);
  return useMemo(() => {
    const paths = docPathsOf(tree, page.path ?? "");
    for (const chip of page.key_chips) paths.add(chip.doc.path);
    for (const block of page.blocks) {
      if (block.kind !== "paragraph") continue;
      for (const sentence of block.sentences) {
        for (const anchor of sentence.rests_on) paths.add(anchor.path);
        for (const mark of sentence.marks) paths.add(mark.doc.path);
      }
    }
    return paths;
  }, [tree, page]);
}

/**
 * On every page replacement (the `page` prop changes and a rebuild arrived, `previous` is a
 * different page), carries the open runs onto the new page by the open-state rule and stores
 * the runs and the notes of the ones that folded. The page present at mount carries nothing:
 * page-state already restored stored runs on load.
 */
function useCarryOnReplace(store: Store, pageKey: string, page: ConceptPage): void {
  const seen = useRef(page);
  useLayoutEffect(() => {
    if (seen.current === page) return;
    seen.current = page;
    const previous = store.get(conceptPageAtom(pageKey)).previous;
    if (!previous || previous === page) return;
    const open = store.get(openRunsAtom(pageKey));
    const result = carryOpenState(previous, page, open, citedLookup(store, page));
    store.set(openRunsAtom(pageKey), result.open);
    if (result.notes.length > 0) {
      store.set(notesAtom(pageKey), (notes) => [...notes, ...result.notes]);
    }
  }, [store, pageKey, page]);
}

type Props = {
  pageKey: string;
  page: ConceptPage;
  /** Called by the `a` key with the focused sentence or passage; absent for read-only browsers. */
  onAttach?: (draft: AttachmentDraft) => void;
};

/**
 * The reading column of concept mode: a document with one tab stop and a roving focus over the
 * page's sentences, the runs opened under them and their passages (section 05). Open runs are
 * view state of this browser tab; a page replacement carries them forward by the source
 * sections they rest on.
 */
export function ReadingColumn({ pageKey, page, onAttach }: Props): JSX.Element {
  const store = useStore();
  const navigate = useNavigate();
  const columnRef = useRef<HTMLDivElement>(null);
  const open = useAtomValue(openRunsAtom(pageKey));
  const notes = useAtomValue(notesAtom(pageKey));
  const cited = useCitedPages(pageKey, page);
  const docPaths = useDocPaths(page);
  const [arrival, setArrival] = useState<Arrival | null>(null);

  useCarryOnReplace(store, pageKey, page);
  const flashing = useSentenceFlash(
    cited.states,
    useCallback((count) => setArrival((last) => ({ count, seq: (last?.seq ?? 0) + 1 })), []),
  );
  const order = useMemo(
    () => readingOrder(page, open, passagesRecord(page, open, cited.lookup)),
    [page, open, cited.lookup],
  );
  const roving = useRovingFocus(columnRef, order, open);
  const { openSentence, foldRun, pendingOpen } = useColumnActions(
    store,
    pageKey,
    page,
    roving.move,
  );
  useScrollAnchor(columnRef, roving, page);
  useOpenScroll(columnRef, pendingOpen, open);

  const openPage = useCallback((target: string) => void navigate(`/${target}`), [navigate]);
  const openAddress = useCallback((address: string) => void navigate(address), [navigate]);
  const context = useMemo<ColumnContextValue>(
    () => ({
      pageKey,
      page,
      open,
      notes,
      flashing,
      lookup: cited.lookup,
      citedPage: cited.citedPage,
      docPaths,
      openSentence,
      foldRun,
      openPage,
      openAddress,
    }),
    [
      pageKey,
      page,
      open,
      notes,
      flashing,
      cited,
      docPaths,
      openSentence,
      foldRun,
      openPage,
      openAddress,
    ],
  );
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) =>
    handleColumnKey(event, roving.current, {
      page,
      pageKey,
      lookup: cited.lookup,
      citedPage: cited.citedPage,
      open,
      order,
      store,
      move: roving.move,
      openSentence,
      foldRun,
      openAddress,
      onAttach,
    });

  return (
    <TooltipProvider>
      <ColumnContext.Provider value={context}>
        <div
          ref={columnRef}
          role="document"
          aria-label="Reading column"
          // One tab stop: the column itself steps aside while focus roves inside it.
          tabIndex={roving.inside ? -1 : 0}
          className="concept-column"
          onKeyDown={onKeyDown}
          onFocus={roving.onFocus}
          onBlur={roving.onBlur}
        >
          <Blocks blocks={page.blocks} />
          <Announcer title={page.title} arrival={arrival} />
        </div>
      </ColumnContext.Provider>
    </TooltipProvider>
  );
}
