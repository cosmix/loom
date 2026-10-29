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
  if (session) return session;
  const concluded = previous?.session;
  return concluded && (concluded.state === "failed" || concluded.state === "ended")
    ? concluded
    : null;
});
