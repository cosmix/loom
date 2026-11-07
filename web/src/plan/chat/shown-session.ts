import { atom } from "jotai";

import type { SessionView } from "@/plan/api/schema-chat";
import { snapshotAtom } from "./model";
// A module cycle with session-home: both sides read the other's atoms only inside getters.
import { screenAtom, type Screen } from "./session-home";

/**
 * The session the state line and the composer present. When the bridge concludes a chat it moves
 * it to `previous` (bridge/ending.rs), so a browser that connects afterwards receives
 * `session: null` with the failed or ended chat in `previous`. It must show the same Failed or
 * Ended state as a tab that watched the chat end, not "No chat" with an enabled composer.
 */
export const shownSessionAtom = atom<SessionView | null>((get) => {
  const { session, previous } = get(snapshotAtom);
  const shown = session ?? previous?.session ?? null;
  if (!shown) return null;
  if (!isConcluded(shown)) return session;
  if (isCleared(shown, get(clearedChatAtom))) return null;
  return concludedHere(shown, get(screenAtom)) ? shown : null;
});

function isConcluded(session: SessionView): boolean {
  return session.state === "failed" || session.state === "ended";
}

/**
 * A concluded chat shows only on its own screen. Elsewhere it is no longer open, so the elsewhere
 * notice does not apply, and showing it would leave that screen with neither a notice nor a
 * composer: another screen gets a fresh chat instead.
 */
function concludedHere(session: SessionView, screen: Screen | null): boolean {
  if (screen === null) return true;
  if (session.feature !== null) return screen.feature === session.feature;
  return session.plan === null || screen.plan === session.plan;
}

/** The concluded chat the user moved past with New chat: its state and turns are no longer shown. */
export const clearedChatAtom = atom<string | null>(null);

/** An active session is never cleared; only a concluded one whose id the user moved past. */
function isCleared(session: SessionView, cleared: string | null): boolean {
  return cleared !== null && session.id === cleared && isConcluded(session);
}
