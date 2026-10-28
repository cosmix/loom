import type { SessionView } from "@/plan/api/schema-chat";
import { authoringAtom, connectionAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import { endChat, startChat } from "./commands";
import { sessionAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { panelCollapsedAtom } from "./prefs";
import { chatSheetOpenAtom } from "./sheet-state";

/** The width at which the panel is a side rail; keep in step with `DESKTOP` in panel.tsx. */
const DESKTOP = "(min-width: 1024px)";
const END_FIRST_CONFIRM =
  "End the current chat first? It will write its open questions into the documents before it stops.";

export function isActiveSession(session: SessionView | null): session is SessionView {
  return session !== null && session.state !== "ended" && session.state !== "failed";
}

/** Asks before a new chat replaces the running one. */
export function confirmEndFirst(): boolean {
  return window.confirm(END_FIRST_CONFIRM);
}

function waitForEnd(
  store: Store,
  id: string,
  signal: AbortSignal,
  retryAction: string,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const cleanups: Array<() => void> = [];
    const finish = (error?: string) => {
      cleanups.splice(0).forEach((stop) => stop());
      if (error) reject(new Error(error));
      else resolve();
    };
    const check = () => {
      const current = store.get(sessionAtom);
      if (signal.aborted) finish("Start cancelled");
      else if (store.get(connectionAtom).phase !== "open")
        finish(`Connection lost; ${retryAction} after reconnecting`);
      else if (current && current.id !== id)
        finish(`${current.holder ?? "Someone"} started a chat first`);
      else if (!isActiveSession(current)) finish();
    };
    cleanups.push(store.sub(sessionAtom, check), store.sub(connectionAtom, check));
    signal.addEventListener("abort", check, { once: true });
    cleanups.push(() => signal.removeEventListener("abort", check));
    check();
  });
}

/**
 * Ends the running chat (unless it is already ending) and resolves once the session is over.
 * Rejects when the start is cancelled, the connection drops, or another browser starts a chat
 * first. `retryAction` completes "Connection lost; <retryAction> after reconnecting".
 */
export async function endCurrentChat(
  store: Store,
  session: SessionView,
  signal: AbortSignal,
  retryAction: string,
): Promise<void> {
  if (session.state !== "ending") await endChat(store);
  await waitForEnd(store, session.id, signal, retryAction);
}

/** The text for a refused start: the holder's name when another browser won the start race. */
export function startError(store: Store, error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const holder = store.get(sessionAtom)?.holder ?? store.get(authoringAtom)?.holder?.name;
  return holder && /chat.*running|already|not_holder|Another browser holds authoring/.test(message)
    ? `${holder} started a chat first`
    : message;
}

/**
 * Starts a chat from an offer for every registered offer handler. With a chat running it asks
 * first, ends it and waits for it to be over. A refusal is shown in the panel, which a start
 * opens in the layout in use so the message is never hidden. One start runs at a time.
 */
export function createOfferStarter(store: Store): {
  start: (offer: string) => Promise<void>;
  dispose: () => void;
} {
  let request: AbortController | null = null;
  const reveal = () => {
    if (window.matchMedia(DESKTOP).matches) store.set(panelCollapsedAtom, false);
    else store.set(chatSheetOpenAtom, true);
  };
  const start = async (offer: string): Promise<void> => {
    if (request) return;
    const running = store.get(sessionAtom);
    if (isActiveSession(running) && !confirmEndFirst()) return;
    const controller = new AbortController();
    request = controller;
    try {
      if (isActiveSession(running))
        await endCurrentChat(store, running, controller.signal, "choose the offer");
      if (controller.signal.aborted) return;
      await startChat(store, offer);
      // Disposed while the start was in flight: the panel is gone, so write and reveal nothing.
      if (controller.signal.aborted) return;
      store.set(offerErrorAtom, null);
    } catch (cause) {
      if (controller.signal.aborted) return;
      store.set(offerErrorAtom, startError(store, cause));
    } finally {
      request = null;
    }
    reveal();
  };
  return { start, dispose: () => request?.abort() };
}
