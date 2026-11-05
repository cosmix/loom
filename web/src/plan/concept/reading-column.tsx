import { useAtomValue, useStore } from "jotai";
import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent,
  type MutableRefObject,
  type RefObject,
} from "react";
import { useNavigate } from "react-router";

import { TooltipProvider } from "@/components/ui/tooltip";
import { pageHref } from "@/plan/api/page-key";
import type { Block, ConceptPage, SourceAnchor } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { visibleControls } from "@/plan/chat/visible-controls";
import { treeAtom } from "@/plan/files/atoms";
import { docPathsOf } from "@/plan/files/tree";
import { normalizeTitle } from "@/plan/markdown/render";
import { sourceMarksAtom } from "@/plan/settings/preferences";
import type { Store } from "@/state/store";

import { Announcer, type Arrival } from "./announce";
import { useColumnActions, type ColumnActions } from "./column-actions";
import { ColumnContext, type ColumnContextValue } from "./column-context";
import { useOpenScroll, useRovingFocus, useScrollAnchor, type Roving } from "./column-focus";
import { handleColumnKey } from "./column-keys";
import { passagesRecord } from "./column-model";
import { useCitedPages, type CitedPages } from "./cited-pages";
import { useSentenceFlash } from "./flash";
import { columnPage } from "./fragments";
import { carryOpenState } from "./open-state";
import { citedLookup, conceptPageAtom } from "./page-state";
import { Blocks } from "./paragraph";
import { sectionInShownDoc } from "./passage-doc";
import { columnOrder } from "./reading-order";
import { firstPassageKeys, SamePassageContext, type SamePassage } from "./same-passage";
import type { ColumnTarget, OpenRun } from "./types";
import { notesAtom, openRunsAtom } from "./view-state";

import "./concept.css";

/** A leading level-1 block repeating the page title, dropped where the page heading shows it. */
function withoutTitleBlock(blocks: Block[], title: string): Block[] {
  const [first, ...rest] = blocks;
  const repeated =
    first?.kind === "heading" &&
    first.level === 1 &&
    normalizeTitle(first.text) === normalizeTitle(title);
  return repeated ? rest : blocks;
}

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
 * page-state already restored stored runs on load. The returned flag is set when the carry
 * wrote, which re-renders the column once more (see `useScrollAnchor`).
 */
function useCarryOnReplace(
  store: Store,
  pageKey: string,
  page: ConceptPage,
): MutableRefObject<boolean> {
  const seen = useRef(page);
  const carried = useRef(false);
  useLayoutEffect(() => {
    if (seen.current === page) return;
    seen.current = page;
    const previous = store.get(conceptPageAtom(pageKey)).previous;
    if (!previous || previous === page) return;
    const open = store.get(openRunsAtom(pageKey));
    const inFile = (anchor: SourceAnchor) => sectionInShownDoc(store, anchor);
    const lookup = citedLookup(store, page);
    const result = carryOpenState(columnPage(previous), columnPage(page), open, lookup, inFile);
    if (JSON.stringify(result.open) !== JSON.stringify(open)) {
      store.set(openRunsAtom(pageKey), result.open);
      carried.current = true;
    }
    if (result.notes.length > 0) {
      store.set(notesAtom(pageKey), (notes) => [...notes, ...result.notes]);
      carried.current = true;
    }
  }, [store, pageKey, page]);
  return carried;
}

type ColumnModel = {
  /** The page with each paragraph's fragments merged: what the column draws and navigates. */
  view: ConceptPage;
  open: OpenRun[];
  cited: CitedPages;
  flashing: ReadonlySet<string>;
  arrival: Arrival | null;
  order: ColumnTarget[];
  roving: Roving;
  actions: ColumnActions;
};

/**
 * The column's state and behaviour: open runs, cited pages, flashes, the reading order with
 * its roving focus, opening and folding, and scrolling. The carry runs before the scroll
 * anchor so the anchor's compensation sees the carry's write in the same commit.
 */
function useColumnModel(
  store: Store,
  columnRef: RefObject<HTMLDivElement | null>,
  pageKey: string,
  page: ConceptPage,
): ColumnModel {
  const open = useAtomValue(openRunsAtom(pageKey));
  const cited = useCitedPages(pageKey, page);
  const [arrival, setArrival] = useState<Arrival | null>(null);
  const carried = useCarryOnReplace(store, pageKey, page);
  const flashing = useSentenceFlash(
    cited.states,
    useCallback((count) => setArrival((last) => ({ count, seq: (last?.seq ?? 0) + 1 })), []),
  );
  const view = columnPage(page);
  const order = useMemo(
    () => columnOrder(view, open, passagesRecord(view, open, cited.lookup), cited.lookup),
    [view, open, cited.lookup],
  );
  const roving = useRovingFocus(columnRef, order, open, { replacement: page, carried });
  const actions = useColumnActions(store, pageKey, view, roving.move);
  useScrollAnchor(columnRef, roving, page, carried);
  useOpenScroll(columnRef, actions.pendingOpen, open);
  return { view, open, cited, flashing, arrival, order, roving, actions };
}

/** The value the column's sentences, runs and passages read through `useColumn`. */
function useContextValue(
  pageKey: string,
  page: ConceptPage,
  model: ColumnModel,
): ColumnContextValue {
  const navigate = useNavigate();
  const notes = useAtomValue(notesAtom(pageKey));
  const docPaths = useDocPaths(page);
  const openPage = useCallback((target: string) => void navigate(pageHref(target)), [navigate]);
  const openAddress = useCallback((address: string) => void navigate(address), [navigate]);
  const { view, open, flashing, cited } = model;
  const { openSentence, foldRun } = model.actions;
  return useMemo<ColumnContextValue>(
    () => ({
      pageKey,
      page: view,
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
      view,
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
}

type Props = {
  pageKey: string;
  page: ConceptPage;
  /**
   * True where a page heading above the column already shows the title (summary pages), so the
   * page's own leading level-1 title block is not rendered twice. Overviews keep it as their H1.
   */
  dropTitle?: boolean;
  /** Called by the `a` key with the focused sentence or passage; absent for read-only browsers. */
  onAttach?: (draft: AttachmentDraft) => void;
};

function columnKeyHandler(
  pageKey: string,
  store: Store,
  model: ColumnModel,
  openAddress: (address: string) => void,
  onAttach: Props["onAttach"],
) {
  return (event: KeyboardEvent<HTMLDivElement>) => {
    handleColumnKey(event, model.roving.current, {
      page: model.view,
      pageKey,
      lookup: model.cited.lookup,
      citedPage: model.cited.citedPage,
      open: model.open,
      order: model.order,
      store,
      move: model.roving.move,
      openSentence: model.actions.openSentence,
      foldRun: model.actions.foldRun,
      openAddress,
      onAttach,
    });
  };
}

/**
 * The reading column has tabbable fold caps and a roving focus over its sentences, runs and
 * passages. Open runs are browser-tab view state; a page replacement carries them forward by
 * the source sections they rest on.
 */
export function ReadingColumn({ pageKey, page, dropTitle = false, onAttach }: Props): JSX.Element {
  const store = useStore();
  const sourceMarks = useAtomValue(sourceMarksAtom);
  const columnRef = useRef<HTMLDivElement>(null);
  const model = useColumnModel(store, columnRef, pageKey, page);
  const context = useContextValue(pageKey, page, model);
  const onKeyDown = columnKeyHandler(pageKey, store, model, context.openAddress, onAttach);
  const samePassage = useMemo<SamePassage>(
    () => ({ first: firstPassageKeys(model.order), goTo: model.roving.move }),
    [model.order, model.roving.move],
  );

  return (
    <TooltipProvider>
      <ColumnContext.Provider value={context}>
        <SamePassageContext.Provider value={samePassage}>
          <div
            ref={columnRef}
            role="document"
            aria-label="Reading column"
            // The column steps aside while focus roves; fold caps keep their own Tab stops.
            tabIndex={model.roving.inside ? -1 : 0}
            className="concept-column"
            data-source-marks={sourceMarks ? undefined : "off"}
            onKeyDown={onKeyDown}
            onFocus={model.roving.onFocus}
            onBlur={model.roving.onBlur}
          >
            <Blocks
              blocks={
                dropTitle ? withoutTitleBlock(model.view.blocks, page.title) : model.view.blocks
              }
            />
            <Announcer title={visibleControls(page.title)} arrival={model.arrival} />
          </div>
        </SamePassageContext.Provider>
      </ColumnContext.Provider>
    </TooltipProvider>
  );
}
