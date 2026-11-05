import { atom } from "jotai";

import type { Store } from "@/state/store";

/** The width at which the panel is a side rail; below it the panel is the sheet. */
export const DESKTOP = "(min-width: 1280px)";

/** Whether the chat sheet (the panel below 1280px) is open. */
export const chatSheetOpenAtom = atom(false);

/**
 * Shows the chat: the side panel is always shown from 1280px, so only a narrower screen needs the
 * sheet opened. Setting the atom at desktop width would open an unasked sheet after a resize.
 */
export function revealChat(store: Store): void {
  if (!window.matchMedia(DESKTOP).matches) store.set(chatSheetOpenAtom, true);
}
