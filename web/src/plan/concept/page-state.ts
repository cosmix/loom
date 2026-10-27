import { useEffect } from "react";
import { atom, useAtomValue, useStore, type PrimitiveAtom } from "jotai";

import { apiGet } from "@/plan/api/client";
import {
  conceptPageSchema,
  type ConceptChanged,
  type ConceptPage,
  type OutOfDate,
} from "@/plan/api/schema-concept";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import { carryStoredRuns, pageSentences, type CitedLookup } from "./open-state";
import { refreshAllDocViews, refreshDocView } from "./passages";
import { notesAtom, openRunsAtom } from "./view-state";

/**
 * One concept page as loaded. `previous` is the page the current one replaced (null on the
 * first load); a mark-only update (out-of-date dashes) sets both to the marked page, so
 * `previous === page` reads as "no rebuild arrived".
 */
export type ConceptPageState = {
  page: ConceptPage | null;
  previous: ConceptPage | null;
  error: string | null;
};

const EMPTY: ConceptPageState = { page: null, previous: null, error: null };
const SOURCE_CHANGED: OutOfDate = { reason: "source_changed", detail: null };
const pageAtoms = new Map<string, PrimitiveAtom<ConceptPageState>>();
const tickets = new WeakMap<Store, Map<string, number>>();

/** Page keys: `product`, `engineering`, `product/<stem>`, `engineering/<slug>`. */
export function conceptPageAtom(key: string): PrimitiveAtom<ConceptPageState> {
  let pageAtom = pageAtoms.get(key);
  if (!pageAtom) {
    pageAtom = atom<ConceptPageState>(EMPTY);
    pageAtoms.set(key, pageAtom);
  }
  return pageAtom;
}

function loadedPage(store: Store, key: string): ConceptPage | null {
  const pageAtom = pageAtoms.get(key);
  return pageAtom ? store.get(pageAtom).page : null;
}

function loadedKeys(store: Store): string[] {
  return [...pageAtoms.keys()].filter((key) => loadedPage(store, key) !== null);
}

/** The page key of a summary id, read from the page's document links (`doc.id` to `doc.page`). */
export function pageKeyOfSummary(page: ConceptPage, summary: string): string {
  const links = [
    ...page.key_chips.map((chip) => chip.doc),
    ...pageSentences(page).flatMap((sentence) => sentence.marks.map((mark) => mark.doc)),
  ];
  return links.find((link) => link.id === summary)?.page ?? summary;
}

/** Resolves an overview's sentence citations against the summary pages loaded in `store`. */
export function citedLookup(store: Store, page: ConceptPage): CitedLookup {
  return (summary, id) => {
    const cited = loadedPage(store, pageKeyOfSummary(page, summary));
    return cited ? pageSentences(cited).find((sentence) => sentence.id === id) : undefined;
  };
}

/**
 * A reload after a rebuild: stored runs on vanished sentences move by overlap or fold.
 * Limit: on a first load the summary pages an overview cites are usually not loaded yet, so
 * the lookup misses and a nested level-1 run is judged on its stored anchors only (kept while
 * its parent still cites its id, else folded). Revisit if nested runs fold after a reload;
 * the fix is to load the cited summary pages before restoring.
 */
function restoreStoredRuns(store: Store, key: string, page: ConceptPage): void {
  const stored = store.get(openRunsAtom(key));
  if (stored.length === 0) return;
  const carried = carryStoredRuns(page, stored, citedLookup(store, page));
  if (!carried) return;
  store.set(openRunsAtom(key), carried.open);
  if (carried.notes.length > 0) store.set(notesAtom(key), (notes) => [...notes, ...carried.notes]);
}

function replacePage(store: Store, key: string, next: ConceptPage): void {
  const pageAtom = conceptPageAtom(key);
  const current = store.get(pageAtom).page;
  if (current === null) restoreStoredRuns(store, key, next);
  store.set(pageAtom, { page: next, previous: current, error: null });
}

function takeTicket(store: Store, key: string): number {
  let byKey = tickets.get(store);
  if (!byKey) {
    byKey = new Map();
    tickets.set(store, byKey);
  }
  const ticket = (byKey.get(key) ?? 0) + 1;
  byKey.set(key, ticket);
  return ticket;
}

function pagePath(key: string): string {
  return `/api/concept/${key.split("/").map(encodeURIComponent).join("/")}`;
}

/**
 * Fetches the pages and applies every answer in one pass, so an overview and the summary
 * pages its runs cite are replaced together and the open-state rule sees them all. A
 * response overtaken by a later request for the same key is dropped; a failed refetch
 * keeps the page and records the error.
 */
export async function loadConceptPages(store: Store, keys: string[]): Promise<void> {
  const requests = [...new Set(keys)].map((key) => ({ key, ticket: takeTicket(store, key) }));
  const results = await Promise.all(
    requests.map(({ key }) => apiGet(pagePath(key), conceptPageSchema)),
  );
  requests.forEach(({ key, ticket }, index) => {
    if (tickets.get(store)?.get(key) !== ticket) return;
    const result = results[index]!;
    if (result.ok) replacePage(store, key, result.value);
    else store.set(conceptPageAtom(key), (state) => ({ ...state, error: result.message }));
  });
}

export function loadConceptPage(store: Store, key: string): Promise<void> {
  return loadConceptPages(store, [key]);
}

/**
 * The page with every mark and key chip of the listed summaries dashed (an existing reason
 * stays); the same object when nothing changed.
 */
export function markOutOfDate(page: ConceptPage, summaries: Set<string>): ConceptPage {
  let changed = false;
  const mark = <T extends { doc: { id: string }; out_of_date: OutOfDate | null }>(item: T): T => {
    if (item.out_of_date !== null || !summaries.has(item.doc.id)) return item;
    changed = true;
    return { ...item, out_of_date: SOURCE_CHANGED };
  };
  const key_chips = page.key_chips.map(mark);
  const blocks = page.blocks.map((block) =>
    block.kind === "paragraph"
      ? { ...block, sentences: block.sentences.map((s) => ({ ...s, marks: s.marks.map(mark) })) }
      : block,
  );
  return changed ? { ...page, key_chips, blocks } : page;
}

function dashOutOfDate(store: Store, summaries: string[]): void {
  if (summaries.length === 0) return;
  const ids = new Set(summaries);
  for (const key of loadedKeys(store)) {
    const pageAtom = conceptPageAtom(key);
    const state = store.get(pageAtom);
    const marked = markOutOfDate(state.page!, ids);
    if (marked !== state.page) store.set(pageAtom, { ...state, page: marked, previous: marked });
  }
}

/**
 * A `concept_changed` delta: dash the out-of-date documents' marks at once, refetch the
 * listed pages that are loaded (an unloaded page is never fetched), and refetch the changed
 * source documents in the passage cache.
 */
export function applyConceptChanged(store: Store, changed: ConceptChanged): void {
  dashOutOfDate(store, changed.out_of_date);
  const keys = changed.pages.filter((key) => loadedPage(store, key) !== null);
  if (keys.length > 0) void loadConceptPages(store, keys);
  for (const path of changed.sources) refreshDocView(store, path);
}

/** After a reconnect deltas were missed: refetch every loaded page and passage. */
export function refetchAllLoaded(store: Store): void {
  const keys = loadedKeys(store);
  if (keys.length > 0) void loadConceptPages(store, keys);
  refreshAllDocViews(store);
}

type PlanSocket = NonNullable<ReturnType<typeof readSocket>>;

function readSocket(store: Store) {
  return store.get(planSocketAtom);
}

function attach(store: Store, socket: PlanSocket | null): () => void {
  // Before the first connect there is no socket; `subscribeConcept` attaches when it appears.
  if (!socket) return () => {};
  const offChanged = socket.subscribe("concept_changed", (message) =>
    applyConceptChanged(store, message),
  );
  const offResync = socket.onResync(() => refetchAllLoaded(store));
  return () => {
    offChanged();
    offResync();
  };
}

/** Follows `concept_changed` and resyncs on the store's plan socket, re-attaching if it changes. */
export function subscribeConcept(store: Store): () => void {
  let detach = attach(store, readSocket(store));
  const unwatch = store.sub(planSocketAtom, () => {
    detach();
    detach = attach(store, readSocket(store));
  });
  return () => {
    unwatch();
    detach();
  };
}

/** Subscribes while a concept page is mounted; `ConceptRoute` calls it. */
export function useConceptSubscription(): void {
  const store = useStore();
  useEffect(() => subscribeConcept(store), [store]);
}

/**
 * The page for `key`. A cached page renders at once and is refetched on mount (deltas are
 * only followed while a concept page is mounted), replacing it through the normal path so
 * the open-state rule runs on the result.
 */
export function useConceptPage(key: string): ConceptPageState {
  const store = useStore();
  const state = useAtomValue(conceptPageAtom(key));
  useEffect(() => {
    void loadConceptPage(store, key);
  }, [store, key]);
  return state;
}
