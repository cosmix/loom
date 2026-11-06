import { atom, useStore } from "jotai";
import { useEffect } from "react";

import { z } from "@/api/zod";
import type { Store } from "@/state/store";
import { projectPref } from "./prefs";

/** The width at which the panel is a side rail; below it the panel is the sheet. */
export const DESKTOP = "(min-width: 1280px)";

/** Whether the chat sheet (the panel below 1280px) is open. */
export const chatSheetOpenAtom = atom(false);

/** The user's own collapse choice for the side panel, stored per project (safeStorage). */
const collapsedPrefAtom = projectPref("panel-collapsed", false, z.boolean());

/**
 * True while a diff view holds the side panel collapsed (`useDiffViewCollapse`); null otherwise.
 * Kept in memory only, so leaving the diff view returns the panel to the user's stored choice.
 */
const diffCollapseAtom = atom<true | null>(null);

/**
 * Whether the side panel (from 1280px) is collapsed. Writing it is the user's choice: it is
 * stored and ends a diff view's hold, so a panel the user expands over a diff stays expanded.
 * The sheet below 1280px ignores it.
 */
export const chatCollapsedAtom = atom(
  (get) => get(diffCollapseAtom) ?? get(collapsedPrefAtom),
  (_get, set, collapsed: boolean) => {
    set(diffCollapseAtom, null);
    set(collapsedPrefAtom, collapsed);
  },
);

/** The desktop side panel's id, for the controls that collapse and expand it. */
export const CHAT_PANEL_ID = "chat-panel";

/**
 * Shows the chat: from 1280px it expands a collapsed side panel, and below it opens the sheet.
 * Setting the sheet atom at desktop width would open an unasked sheet after a resize.
 */
export function revealChat(store: Store): void {
  if (!window.matchMedia(DESKTOP).matches) store.set(chatSheetOpenAtom, true);
  else if (store.get(chatCollapsedAtom)) store.set(chatCollapsedAtom, false);
}

/**
 * Collapses the side panel once when a diff screen mounts, so the diff has the page's width, and
 * releases it on leaving, which returns the panel to the user's stored choice. A user who expands
 * the panel over the diff ends the hold (`chatCollapsedAtom`), so it stays expanded until they
 * leave and come back.
 */
export function useDiffViewCollapse(): void {
  const store = useStore();
  useEffect(() => {
    store.set(diffCollapseAtom, true);
    return () => store.set(diffCollapseAtom, null);
  }, [store]);
}
