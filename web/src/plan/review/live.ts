import { useEffect } from "react";
import { atom, useStore } from "jotai";
import { getHashes, getOpenQuestions, type HashesResponse } from "./api";
import { planSocketAtom } from "@/plan/state/atoms";

export const openQuestionsAtom = atom<Record<string, number>>({});
export const hashesAtom = atom<HashesResponse | null>(null);

export function useReviewLive(): void {
  const store = useStore();
  useEffect(() => {
    let active = true;
    let countsVersion = 0;
    let hashesVersion = 0;
    const refresh = () => {
      const countsStarted = ++countsVersion;
      const hashesStarted = ++hashesVersion;
      void getOpenQuestions().then((result) => {
        if (active && result.ok && countsStarted === countsVersion) {
          store.set(openQuestionsAtom, result.value.counts);
        }
      });
      void getHashes().then((result) => {
        if (active && result.ok && hashesStarted === hashesVersion) {
          store.set(hashesAtom, result.value);
        }
      });
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
      const offResync = socket.onResync(refresh);
      return () => {
        offReview();
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
