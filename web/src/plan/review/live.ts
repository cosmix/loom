import { useEffect } from "react";
import { atom, useStore } from "jotai";
import type { ApiResult } from "@/plan/api/client";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import { getHashes, getOpenQuestions, type HashesResponse } from "./api";

export const openQuestionsAtom = atom<Record<string, number>>({});
export const hashesAtom = atom<HashesResponse | null>(null);

type HashesResult = ApiResult<HashesResponse>;
type Waiter = (result: HashesResult) => void;
// `next` is the waiters of the one follow-up load, or null while none is wanted.
type HashLoad = { loading: boolean; next: Waiter[] | null };
const hashLoads = new WeakMap<Store, HashLoad>();

function startHashLoad(store: Store, load: HashLoad, waiters: Waiter[]): void {
  load.loading = true;
  void getHashes().then((result) => {
    load.loading = false;
    if (result.ok) store.set(hashesAtom, result.value);
    for (const waiter of waiters) waiter(result);
    const next = load.next;
    load.next = null;
    if (next) startHashLoad(store, load, next);
  });
}

/**
 * Loads `/api/hashes` into `hashesAtom` and resolves with the response it applied. Loads run one at
 * a time per store, shared by every caller: a request that arrives while one is in flight (a
 * `doc_changed` burst is one frame per document of a chat turn) asks for exactly one more load
 * after it, so an older response never overwrites a newer one and a burst costs two requests, not
 * one per frame. A caller always gets a response read after its call, never one already in flight.
 */
export function loadHashes(store: Store): Promise<HashesResult> {
  const load: HashLoad = hashLoads.get(store) ?? { loading: false, next: null };
  hashLoads.set(store, load);
  return new Promise((resolve) => {
    if (load.loading) {
      (load.next ??= []).push(resolve);
    } else {
      startHashLoad(store, load, [resolve]);
    }
  });
}

export function useReviewLive(): void {
  const store = useStore();
  useEffect(() => {
    let active = true;
    let countsVersion = 0;
    const refreshHashes = () => void loadHashes(store);
    const refresh = () => {
      const countsStarted = ++countsVersion;
      void getOpenQuestions().then((result) => {
        if (active && result.ok && countsStarted === countsVersion) {
          store.set(openQuestionsAtom, result.value.counts);
        }
      });
      refreshHashes();
    };
    const attach = () => {
      const socket = store.get(planSocketAtom);
      if (!socket) return () => {};
      const offReview = socket.subscribe("review", (message) => {
        if (message.kind === "open_questions") {
          countsVersion++;
          store.set(openQuestionsAtom, message.counts);
        }
      });
      // The frame's hash is the same `normalized_hash` `/api/hashes` lists, but it is null
      // for any path without a held text (non-markdown, outside doc/, deleted) while that
      // endpoint omits such paths and also carries `head`; refetching keeps one source of truth.
      const offDocs = socket.subscribe("doc_changed", refreshHashes);
      const offResync = socket.onResync(refresh);
      return () => {
        offReview();
        offDocs();
        offResync();
      };
    };
    let detach = attach();
    const unwatch = store.sub(planSocketAtom, () => {
      detach();
      detach = attach();
    });
    refresh();
    return () => {
      active = false;
      unwatch();
      detach();
    };
  }, [store]);
}
