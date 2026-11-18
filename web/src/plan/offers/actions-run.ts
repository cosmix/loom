import { atom, type createStore } from "jotai";
import { registerOfferAction } from "@/plan/offers/actions";

export const runDialogAtom = atom<{ open: boolean; plan: string | null }>({
  open: false,
  plan: null,
});

export function registerRunOfferAction(store: ReturnType<typeof createStore>): () => void {
  return registerOfferAction("start_run", (offer) => {
    store.set(runDialogAtom, { open: true, plan: offer.target });
  });
}
