import { atom } from "jotai";

import type { SessionView } from "@/plan/api/schema-chat";
import { snapshotAtom } from "./model";

/**
 * The session the state line and the composer present. When the bridge concludes a chat it moves
 * it to `previous` (bridge/ending.rs), so a browser that connects afterwards receives
 * `session: null` with the failed or ended chat in `previous`. It must show the same Failed or
 * Ended state as a tab that watched the chat end, not "No chat" with an enabled composer.
 */
export const shownSessionAtom = atom<SessionView | null>((get) => {
  const { session, previous } = get(snapshotAtom);
  const cleared = get(clearedChatAtom);
  if (session) return isCleared(session, cleared) ? null : session;
  const concluded = previous?.session;
  if (!concluded || (concluded.state !== "failed" && concluded.state !== "ended")) return null;
  return isCleared(concluded, cleared) ? null : concluded;
});

/** The concluded chat the user moved past with New chat: its state and turns are no longer shown. */
export const clearedChatAtom = atom<string | null>(null);

/** An active session is never cleared; only a concluded one whose id the user moved past. */
function isCleared(session: SessionView, cleared: string | null): boolean {
  return (
    cleared !== null &&
    session.id === cleared &&
    (session.state === "failed" || session.state === "ended")
  );
}
