import { atom } from "jotai";

/**
 * Why the last offer start was refused. Offer registration writes it; the panel shows it under
 * the state line until the reader dismisses it or the next start succeeds.
 */
export const offerErrorAtom = atom<string | null>(null);
