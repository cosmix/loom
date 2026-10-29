import { atom, type createStore } from "jotai";
import { registerOfferAction } from "@/plan/offers/actions";

export const pressureDialogAtom = atom<{ open: boolean; plan: string | null }>({
  open: false,
  plan: null,
});

export function registerPressureOfferAction(store: ReturnType<typeof createStore>): () => void {
  return registerOfferAction("pressure_test", (offer) => {
    store.set(pressureDialogAtom, { open: true, plan: offer.target });
  });
}
