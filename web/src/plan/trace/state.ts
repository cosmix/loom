import { useEffect } from "react";
import { atom, useAtomValue, useStore, type PrimitiveAtom } from "jotai";

import { apiGet } from "@/plan/api/client";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

/**
 * One feature's trace as loaded. A failed refetch keeps `trace` and records `error`; `status` is
 * the HTTP status behind it (0 when the request never got an answer).
 */
export type TraceState = { trace: TraceView | null; error: string | null; status?: number };

const EMPTY: TraceState = { trace: null, error: null };
const traceAtoms = new Map<string, PrimitiveAtom<TraceState>>();
/** What `useTrace` reads for no feature (the checklist has not arrived); nothing ever sets it. */
const NO_TRACE = atom<TraceState>(EMPTY);
const tickets = new WeakMap<Store, Map<string, number>>();

/** The trace of one feature: fetched on first use, refetched while `useTrace` is mounted. */
export function traceAtom(feature: string): PrimitiveAtom<TraceState> {
  let traceAtomOfFeature = traceAtoms.get(feature);
  if (!traceAtomOfFeature) {
    traceAtomOfFeature = atom<TraceState>(EMPTY);
    traceAtoms.set(feature, traceAtomOfFeature);
  }
  return traceAtomOfFeature;
}

function takeTicket(store: Store, feature: string): number {
  let byFeature = tickets.get(store);
  if (!byFeature) {
    byFeature = new Map();
    tickets.set(store, byFeature);
  }
  const ticket = (byFeature.get(feature) ?? 0) + 1;
  byFeature.set(feature, ticket);
  return ticket;
}

function requested(store: Store, feature: string): boolean {
  return tickets.get(store)?.has(feature) ?? false;
}

/**
 * A failed fetch keeps a loaded trace and records the error, unless the server says the feature
 * is gone (404): then the cached trace goes too.
 */
function failedState(state: TraceState, failure: { status: number; message: string }): TraceState {
  const { status, message } = failure;
  return status === 404
    ? { trace: null, error: message, status }
    : { ...state, error: message, status };
}

/**
 * Fetches a feature's trace. A response overtaken by a later request for the same feature is
 * dropped, so a slow answer never replaces a newer one.
 */
export async function loadTrace(store: Store, feature: string): Promise<void> {
  const ticket = takeTicket(store, feature);
  const result = await apiGet(`/api/trace/${encodeURIComponent(feature)}`, traceViewSchema);
  if (tickets.get(store)?.get(feature) !== ticket) return;
  if (result.ok) store.set(traceAtom(feature), { trace: result.value, error: null });
  else store.set(traceAtom(feature), (state) => failedState(state, result));
}

/** Refetches every feature this store has asked for. */
function refetchRequested(store: Store): void {
  for (const feature of traceAtoms.keys()) {
    if (requested(store, feature)) void loadTrace(store, feature);
  }
}

type PlanSocket = NonNullable<ReturnType<typeof readSocket>>;

function readSocket(store: Store) {
  return store.get(planSocketAtom);
}

function attach(store: Store, socket: PlanSocket | null): () => void {
  // Before the first connect there is no socket; `subscribeTrace` attaches when it appears.
  if (!socket) return () => {};
  const offChanged = socket.subscribe("trace_changed", ({ feature }) => {
    if (requested(store, feature)) void loadTrace(store, feature);
  });
  const offResync = socket.onResync(() => refetchRequested(store));
  return () => {
    offChanged();
    offResync();
  };
}

/** Follows `trace_changed` and resyncs on the store's plan socket, re-attaching if it changes. */
export function subscribeTrace(store: Store): () => void {
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

/** One socket subscription per store, shared by every mounted `useTrace`. */
const watchers = new WeakMap<Store, { users: number; stop: () => void }>();

function watch(store: Store): () => void {
  let watcher = watchers.get(store);
  if (!watcher) {
    watcher = { users: 0, stop: subscribeTrace(store) };
    watchers.set(store, watcher);
  }
  const held = watcher;
  held.users += 1;
  return () => {
    held.users -= 1;
    if (held.users > 0) return;
    held.stop();
    watchers.delete(store);
  };
}

/**
 * The trace of `feature` (an empty string reads nothing and fetches nothing). While any
 * component holds it the plan socket is followed. The first holder refetches on mount, since
 * deltas were missed while none was mounted; later holders reuse the loaded view.
 */
export function useTrace(feature: string): TraceState {
  const store = useStore();
  const state = useAtomValue(feature === "" ? NO_TRACE : traceAtom(feature));
  useEffect(() => {
    if (feature === "") return;
    const first = !watchers.has(store);
    const release = watch(store);
    if (first || !requested(store, feature)) void loadTrace(store, feature);
    return release;
  }, [store, feature]);
  return state;
}
