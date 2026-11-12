import { useStore } from "jotai";
import { lazy, useEffect, type JSX } from "react";

import { installPressureSubscription } from "@/plan/chat/pressure/use-pressure";
import { registerPressureOfferAction } from "@/plan/offers/actions-pressure";
import { registerRunOfferAction } from "@/plan/offers/actions-run";
import { fetchRunState, installRunSubscription } from "@/plan/run/use-run";
import { OptionalChunk } from "@/plan/ui/optional-chunk";

// The dialogs are a separate chunk, fetched after the header renders: they open only on an offer
// or a handoff, and their steps and diagrams kept plan.js over vite's 500 kB chunk warning.
// Revisit if a dialog must open before the chunk arrives.
const HandoffDialogs = lazy(() =>
  import("@/plan/run/handoff-dialogs").then((module) => ({ default: module.HandoffDialogs })),
);

export function HandoffMounts(): JSX.Element {
  const store = useStore();
  useEffect(() => {
    const cleanups = [
      installPressureSubscription(store),
      installRunSubscription(store),
      registerPressureOfferAction(store),
      registerRunOfferAction(store),
    ];
    void fetchRunState(store);
    return () => cleanups.forEach((cleanup) => cleanup());
  }, [store]);
  return (
    <OptionalChunk>
      <HandoffDialogs />
    </OptionalChunk>
  );
}
