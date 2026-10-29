import { useStore } from "jotai";
import { useEffect, type JSX } from "react";

import { PressureDialog } from "@/plan/chat/pressure/pressure-dialog";
import { installPressureSubscription } from "@/plan/chat/pressure/use-pressure";
import { registerPressureOfferAction } from "@/plan/offers/actions-pressure";
import { registerRunOfferAction } from "@/plan/offers/actions-run";
import { RunDialog } from "@/plan/run/run-dialog";
import { fetchRunState, installRunSubscription } from "@/plan/run/use-run";

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
    <>
      <PressureDialog />
      <RunDialog />
    </>
  );
}
