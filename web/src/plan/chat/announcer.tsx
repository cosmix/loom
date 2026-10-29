import { useAtomValue, useStore } from "jotai/react";
import { atom } from "jotai/vanilla";
import { useEffect, useRef, type JSX } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { pendingPermissionsAtom } from "./model";
import { shownSessionAtom } from "./shown-session";
import { Untrusted } from "./visible-controls";

/**
 * Pause between emptying the assertive region and writing the next notice. A second request for
 * the same tool has the same words as the first; without the empty text in between, the region
 * would not change and a screen reader would say nothing.
 */
const REARM_MS = 50;

/** Request ids already announced. Per store, so the regions can move without repeating one. */
const announcedAtom = atom<readonly string[]>([]);
/** What the assertive region says; empty between arrivals. */
const noticeAtom = atom("");

/**
 * What the polite region says when a chat concludes; null for every other state. The text changes
 * only when the outcome does, so a reconnect that replays the same session announces nothing.
 */
function concludedWords(session: SessionView | null): string | null {
  if (session?.state === "ended") return "Chat ended";
  if (session?.state !== "failed") return null;
  return session.failure?.kind === "spawn"
    ? "The session could not start"
    : "The session stopped unexpectedly";
}

/**
 * Writes the notice for each permission request once per page, however many times a snapshot
 * replays it. Runs in the one component that stays mounted, so moving the regions between the page
 * and the sheet never announces a request again or drops one.
 */
function useAnnounceArrivals(): void {
  const store = useStore();
  const pending = useAtomValue(pendingPermissionsAtom);
  // Tools that arrived but are not yet written: arrivals inside one pause share a notice.
  const waiting = useRef<string[]>([]);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    if (pending.length === 0) {
      // Cleared once nothing waits, so a later request for the same tool is a change again.
      clearTimeout(timer.current);
      waiting.current = [];
      store.set(noticeAtom, "");
      return;
    }
    const announced = store.get(announcedAtom);
    const fresh = pending.filter((item) => !announced.includes(item.request_id));
    if (fresh.length === 0) return;
    store.set(announcedAtom, [...announced, ...fresh.map((item) => item.request_id)]);
    waiting.current.push(...fresh.map((item) => item.tool));
    clearTimeout(timer.current);
    store.set(noticeAtom, "");
    // Not cancelled on unmount: the write only sets an atom, and StrictMode's remount would
    // otherwise drop a request that is already marked as announced.
    timer.current = setTimeout(() => {
      store.set(noticeAtom, `Needs a decision: ${waiting.current.join(", ")}`);
      waiting.current = [];
    }, REARM_MS);
  }, [pending, store]);
}

/**
 * The live regions themselves. The regions exist once a chat exists (its `starting` state comes
 * before any permission request or conclusion), so the page carries no empty live regions when
 * there is no chat.
 */
export function AnnouncerRegions(): JSX.Element | null {
  const shown = useAtomValue(shownSessionAtom);
  const notice = useAtomValue(noticeAtom);
  if (!shown) return null;
  return (
    <>
      <div className="sr-only" aria-live="assertive" aria-atomic="true" data-chat-announcer="alert">
        <Untrusted text={notice} />
      </div>
      <div className="sr-only" aria-live="polite" aria-atomic="true" data-chat-announcer="status">
        <Untrusted text={concludedWords(shown) ?? ""} />
      </div>
    </>
  );
}

/**
 * Announcements that must not depend on the panel being open. The chat panel unmounts its state
 * line and composer while it is a collapsed rail or a closed sheet, so a permission request or a
 * failed or ended chat would otherwise arrive in silence. Mounted beside the panel with the other
 * registrations; the state line's own polite region leaves these states to it.
 *
 * A modal sheet marks everything outside itself `aria-hidden`, this component's page included, so
 * while the sheet is open (`inSheet`) the regions render inside it instead: the sheet mounts
 * `AnnouncerRegions` and this component renders none. Exactly one pair exists at a time.
 */
export function ChatAnnouncer({ inSheet }: { inSheet: boolean }): JSX.Element | null {
  useAnnounceArrivals();
  return inSheet ? null : <AnnouncerRegions />;
}
