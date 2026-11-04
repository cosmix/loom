import { atom } from "jotai/vanilla";

import type { KickoffDraftView } from "@/plan/api/schema-kickoff";
import { sessionAtom } from "@/plan/chat/model";
import type { Store } from "@/state/store";

/**
 * The kickoff this browser tab just started, held only until `KickoffRoute` has moved the tab to
 * `/new?session=<id>`. It is navigation state, never persisted, and never says which kickoff the
 * route shows: the query id does.
 */
export const kickoffSessionAtom = atom<string | null>(null);

/** Whether the chat session is the kickoff the address names (`/new?session=<id>`). */
export function isKickoffSession(sessionId: string | null, requested: string | null): boolean {
  return sessionId != null && sessionId === requested;
}

/**
 * Resolves once a session other than `previousId` is in the snapshot (recording it as this tab's
 * kickoff), the wait is aborted, or `onTimeout` fires after five seconds.
 */
export function waitForKickoffSession(
  store: Store,
  previousId: string | null,
  signal: AbortSignal,
  onTimeout: () => void,
) {
  return new Promise<void>((resolve) => {
    const cleanups: Array<() => void> = [];
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      cleanups.splice(0).forEach((stop) => stop());
      resolve();
    };
    const check = () => {
      if (signal.aborted) return finish();
      const session = store.get(sessionAtom);
      // An ended session may remain in the snapshot until its replacement arrives.
      if (session !== null && session.id !== previousId) {
        store.set(kickoffSessionAtom, session.id);
        finish();
      }
    };
    cleanups.push(store.sub(sessionAtom, check));
    // A slow snapshot can still arrive after the UI offers a retry.
    const timeout = setTimeout(onTimeout, 5000);
    cleanups.push(() => clearTimeout(timeout));
    const abort = () => finish();
    signal.addEventListener("abort", abort, { once: true });
    cleanups.push(() => signal.removeEventListener("abort", abort));
    check();
  });
}

/** The date a draft was started, in the reader's short locale form; empty when unparseable. */
function startedOn(startedAt: string): string {
  const started = new Date(startedAt);
  if (Number.isNaN(started.getTime())) return "";
  return started.toLocaleDateString(undefined, { dateStyle: "short" });
}

/** A draft's name, or "Untitled" with the date it was started once the agent has named nothing. */
export function draftTitle(draft: Pick<KickoffDraftView, "name" | "started_at">): string {
  if (draft.name) return draft.name;
  const date = startedOn(draft.started_at);
  return date === "" ? "Untitled" : `Untitled · ${date}`;
}

/** The address of a kickoff's screen: its conversation while live, its draft once ended. */
export function kickoffHref(session: string): string {
  return `/new?session=${encodeURIComponent(session)}`;
}
