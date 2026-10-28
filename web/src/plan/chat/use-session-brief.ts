import { useEffect, useState } from "react";

import type { SessionBriefView } from "@/plan/api/schema-session-brief";
import { getSessionBrief } from "./commands";

export type BriefState =
  | { status: "loading" }
  | { status: "ready"; brief: SessionBriefView }
  | { status: "unavailable" }
  | { status: "error"; message: string };

/** A session's brief is rendered shortly after it starts; 404 `unknown_session` means not yet. */
const MAX_ATTEMPTS = 30;
const RETRY_MS = 1000;

/** Loads a session's brief, retrying once a second while the server has not rendered it. */
export function useSessionBrief(sessionId: string | null): BriefState {
  const [state, setState] = useState<BriefState>({ status: "loading" });
  useEffect(() => {
    if (sessionId === null) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let attempts = 0;
    setState({ status: "loading" });
    async function load(id: string) {
      attempts += 1;
      const result = await getSessionBrief(id);
      if (!active) return;
      if (result.ok) {
        setState({ status: "ready", brief: result.value });
      } else if (result.status === 404 && result.error === "unknown_session") {
        if (attempts < MAX_ATTEMPTS) timer = setTimeout(() => void load(id), RETRY_MS);
        else setState({ status: "unavailable" });
      } else {
        setState({ status: "error", message: result.message });
      }
    }
    void load(sessionId);
    return () => {
      active = false;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [sessionId]);
  return state;
}
