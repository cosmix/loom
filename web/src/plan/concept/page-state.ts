import { useEffect, useRef } from "react";
import { atom, useAtomValue, useStore, type PrimitiveAtom } from "jotai";

import { apiGet } from "@/plan/api/client";
import { isConceptPageKey, overviewKey, parsePageKey } from "@/plan/api/page-key";
import {
  conceptPageSchema,
  type ConceptChanged,
  type ConceptPage,
  type OutOfDate,
} from "@/plan/api/schema-concept";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import { summaryIdOfKey } from "./column-context";
import { columnPage } from "./fragments";
import { carryStoredRuns, citableSentences, pageSentences, type CitedLookup } from "./open-state";
import { refreshAllDocViews, refreshDocView } from "./passages";
import { notesAtom, openRunsAtom } from "./view-state";

/**
 * One concept page as loaded. `previous` is the page the current one replaced (null on the
 * first load); a mark-only update (out-of-date dashes) sets both to the marked page, so
 * `previous === page` reads as "no rebuild arrived". `status` is the HTTP status of the failed
 * fetch behind `error` (0 when the request never got an answer).
 */
export type ConceptPageState = {
  page: ConceptPage | null;
  previous: ConceptPage | null;
  error: string | null;
  status?: number;
};

const EMPTY: ConceptPageState = { page: null, previous: null, error: null };
const SOURCE_CHANGED: OutOfDate = { reason: "source_changed", detail: null };
const pageAtoms = new Map<string, PrimitiveAtom<ConceptPageState>>();
/** What `useConceptPage` reads for a key that is no page key; nothing ever sets it. */
const NO_PAGE = atom<ConceptPageState>(EMPTY);
const tickets = new WeakMap<Store, Map<string, number>>();

/** Page keys: `overview/<feature>/<scope>`, `<feature>/<scope>/<doc>`, `plans/<doc>`. */
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

/**
 * The page key of a summary id when the page names that document: its key chips and marks are
 * the server's document links (`doc.id` to `doc.page`), and the server lists only documents that
 * have a summary page. Null for a citation of a document the page cannot name (a plan that no
 * longer exists), which has no page to fetch.
 */
export function linkedPageKey(page: ConceptPage, summary: string): string | null {
  const links = [
    ...page.key_chips.map((chip) => chip.doc),
    ...pageSentences(page).flatMap((sentence) => sentence.marks.map((mark) => mark.doc)),
  ];
  return links.find((link) => link.id === summary)?.page ?? null;
}

/** The page key of a summary id: the linked page, else the id itself. */
export function pageKeyOfSummary(page: ConceptPage, summary: string): string {
  return linkedPageKey(page, summary) ?? summary;
}

/**
 * Resolves an overview's sentence citations against the summary pages loaded in `store`; the
 * page's own summary id resolves against `page` itself (a two-level plan cites its detail).
 */
export function citedLookup(store: Store, page: ConceptPage): CitedLookup {
  const own = summaryIdOfKey(page.key);
  return (summary, id) => {
    const cited = summary === own ? page : loadedPage(store, pageKeyOfSummary(page, summary));
    return cited ? citableSentences(cited).find((sentence) => sentence.id === id) : undefined;
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
  const carried = carryStoredRuns(columnPage(page), stored, citedLookup(store, page));
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

/**
 * The request path of a page key; null for a key the server would reject, so nothing is sent.
 * A valid key holds only unreserved characters and `/`, which need no escaping.
 */
function pagePath(key: string): string | null {
  return isConceptPageKey(key) ? `/api/concept/${key}` : null;
}

/** 404, or 400 for a key the server cannot parse: the page does not exist. */
const MISSING_STATUSES: readonly number[] = [404, 400];

/** Whether the last fetch said the page does not exist. */
export function isMissing(state: ConceptPageState): boolean {
  return state.error !== null && MISSING_STATUSES.includes(state.status ?? 0);
}

/**
 * A failed fetch keeps a loaded page and records the error, unless the server says the page
 * is gone: then the cached page goes too and the route shows the not-found state.
 */
function failedState(
  state: ConceptPageState,
  failure: { status: number; message: string },
): ConceptPageState {
  const { status, message } = failure;
  if (MISSING_STATUSES.includes(status)) {
    return { page: null, previous: null, error: message, status };
  }
  return { ...state, error: message, status };
}

/**
 * Fetches the pages and applies every answer in one pass, so an overview and the summary
 * pages its runs cite are replaced together and the open-state rule sees them all. A
 * response overtaken by a later request for the same key is dropped; a failed refetch
 * keeps the page and records the error (see `failedState`). A key that is not a page key is
 * never fetched.
 */
export async function loadConceptPages(store: Store, keys: string[]): Promise<void> {
  const requests = [...new Set(keys)].flatMap((key) => {
    const path = pagePath(key);
    return path === null ? [] : [{ key, path, ticket: takeTicket(store, key) }];
  });
  const results = await Promise.all(requests.map(({ path }) => apiGet(path, conceptPageSchema)));
  requests.forEach(({ key, ticket }, index) => {
    if (tickets.get(store)?.get(key) !== ticket) return;
    const result = results[index]!;
    if (result.ok) replacePage(store, key, result.value);
    else store.set(conceptPageAtom(key), (state) => failedState(state, result));
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
  const detail = page.detail?.map((s) => ({ ...s, marks: s.marks.map(mark) }));
  return changed ? { ...page, key_chips, blocks, ...(detail && { detail }) } : page;
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
 * The pages a delta refetches: the loaded pages it names, plus the loaded siblings whose key
 * row embeds a chip the delta changed. The server names the product overview when a section's
 * summary state or the key row changes, and the metaplan page when any plan chip's state
 * changes. Every product page of that feature embeds the product key row, and every plan page
 * of that feature embeds plan chips, so a sibling the delta does not name is stale too. Each
 * page shows its own fresh response, ordered by its own ticket; no state is copied from one
 * page to another (copying across keys has no order, so an older response could overwrite a
 * newer dash).
 */
function pagesToRefetch(store: Store, named: string[]): string[] {
  const loaded = loadedKeys(store);
  const overviews = new Set(named.filter((key) => parsePageKey(key)?.kind === "overview"));
  const inScope = (key: string) => {
    const parts = parsePageKey(key);
    if (parts === null || parts.kind === "standalone") return false;
    if (parts.scope === "product") return overviews.has(overviewKey(parts.feature, parts.scope));
    // A plan page's chips name sibling plans, which the metaplan page's delta covers.
    return parts.scope === "engineering" && named.includes(overviewKey(parts.feature, "metaplan"));
  };
  return [...named.filter((key) => loaded.includes(key)), ...loaded.filter(inScope)];
}

/**
 * A `concept_changed` delta: dash the out-of-date documents' marks at once, refetch the
 * pages it affects that are loaded (an unloaded page is never fetched; see `pagesToRefetch`),
 * and refetch the changed source documents in the passage cache.
 */
export function applyConceptChanged(store: Store, changed: ConceptChanged): void {
  dashOutOfDate(store, changed.out_of_date);
  const keys = pagesToRefetch(store, changed.pages);
  if (keys.length > 0) void loadConceptPages(store, keys);
  for (const path of changed.sources) refreshDocView(store, path);
}

/**
 * After a reconnect, or on mounting after time away, deltas were missed: refetch every loaded
 * page (and `also`, loaded or not, in the same pass) and every passage.
 */
export function refetchAllLoaded(store: Store, also: string[] = []): void {
  const keys = [...also, ...loadedKeys(store)];
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
 * The page for `key`. A cached page renders at once. Deltas are followed only while a concept
 * page is mounted, so on mount every cached page and passage is refetched, as after a
 * reconnect (a cited summary page or an opened passage may have changed while away); a later
 * key change happens while subscribed and refetches only its own page. Pages are replaced
 * through the normal path so the open-state rule runs on the result. A key that is not a page
 * key reads a shared empty atom, so a mistyped URL adds nothing to the page atom map.
 */
export function useConceptPage(key: string): ConceptPageState {
  const store = useStore();
  const state = useAtomValue(isConceptPageKey(key) ? conceptPageAtom(key) : NO_PAGE);
  const mounted = useRef(false);
  useEffect(() => {
    if (mounted.current) void loadConceptPage(store, key);
    else refetchAllLoaded(store, [key]);
    mounted.current = true;
  }, [store, key]);
  return state;
}
