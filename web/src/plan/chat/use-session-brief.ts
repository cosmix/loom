import { useStore } from "jotai";
import { useEffect, useState } from "react";

import type { SessionBriefView } from "@/plan/api/schema-session-brief";
import type { Store } from "@/state/store";
import { getSessionBrief } from "./commands";
import { sessionAtom } from "./model";

export type BriefState =
  | { status: "loading" }
  | { status: "ready"; brief: SessionBriefView }
  | { status: "unavailable" }
  | { status: "error"; message: string };

const LOADING: BriefState = { status: "loading" };
const MAX_ATTEMPTS = 30;
const RETRY_MS = 1000;

/**
 * Whether a 404 may yet turn into a brief. The server remembers one only for an authoring chat, in
 * memory: a pressure test never has one and a restart loses it, so for those the 404 is final.
 */
function mayStillRender(store: Store, id: string): boolean {
  const session = store.get(sessionAtom);
  return session?.id === id && session.kind === "authoring" && session.state === "starting";
}

/** Loads a session's brief, retrying once a second while an authoring chat is still starting. */
export function useSessionBrief(sessionId: string | null): BriefState {
  const store = useStore();
  // Keyed by session, so a new session never paints with the previous one's brief.
  const [loaded, setLoaded] = useState<{ id: string; state: BriefState } | null>(null);
  useEffect(() => {
    if (sessionId === null) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let attempts = 0;
    async function load(id: string) {
      attempts += 1;
      const result = await getSessionBrief(id);
      if (!active) return;
      if (result.ok) {
        setLoaded({ id, state: { status: "ready", brief: result.value } });
      } else if (result.status === 404 && result.error === "unknown_session") {
        if (attempts < MAX_ATTEMPTS && mayStillRender(store, id))
          timer = setTimeout(() => void load(id), RETRY_MS);
        else setLoaded({ id, state: { status: "unavailable" } });
      } else {
        setLoaded({ id, state: { status: "error", message: result.message } });
      }
    }
    void load(sessionId);
    return () => {
      active = false;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [sessionId, store]);
  return loaded?.id === sessionId ? loaded.state : LOADING;
}
