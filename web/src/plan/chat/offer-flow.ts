import type { SessionView } from "@/plan/api/schema-chat";
import { authoringAtom, connectionAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import { endChat, startChat } from "./commands";
import { userWrote } from "./closing";
import { sessionAtom, turnsAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import {
  elsewhereSessionAtom,
  isActiveSession,
  startOwnerAtom,
  type SessionHome,
} from "./session-home";
import { revealChat } from "./sheet-state";
const END_FIRST_CONFIRM =
  "End the current chat first? It will write its open questions into the documents before it stops.";

export function elsewhereRefusal(home: SessionHome): string {
  return `Finish the session in ${home.title} first.`;
}

/**
 * Why a screen with no panel (`/`, an empty `/new`) refuses a start, or null with no chat open. Such
 * a screen never shows a session, so it never offers to end one: not even one with no home, which
 * reads as here on every screen that has a panel.
 */
export function noPanelRefusal(store: Store): string | null {
  if (!isActiveSession(store.get(sessionAtom))) return null;
  const elsewhere = store.get(elsewhereSessionAtom);
  return elsewhere ? elsewhereRefusal(elsewhere) : "Finish the open chat first.";
}

/**
 * Asks before a new chat replaces the running one on this screen. It asks nothing when the chat is
 * already ending or the user never wrote to it: no closing write runs, so no open questions are
 * written and there is nothing to warn about.
 */
export function confirmEndFirst(store: Store): boolean {
  const running = store.get(sessionAtom);
  if (running?.state === "ending" || !userWrote(store.get(turnsAtom))) return true;
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
 * Starts a chat from an offer for every registered offer handler. With a chat running on this
 * screen it asks first, ends it and waits for it to be over; with one running elsewhere it refuses
 * without asking. A refusal is shown in the panel; where there is no side panel a start opens the
 * sheet so the message is never hidden. One start runs at a time.
 */
export function createOfferStarter(store: Store): {
  start: (offer: string, feature?: string, text?: string) => Promise<void>;
  dispose: () => void;
} {
  let request: AbortController | null = null;
  const reveal = () => revealChat(store);
  const start = async (offer: string, feature?: string, text?: string): Promise<void> => {
    if (request) return;
    // The end wait can last minutes: the chat belongs to the screen the offer was chosen on.
    // `plan_follow_ups` names no feature on the wire, so the screen's applies.
    const owner = store.get(startOwnerAtom);
    const ownerFeature = feature ?? owner.feature;
    const plan = feature === undefined ? owner.plan : undefined;
    const elsewhere = store.get(elsewhereSessionAtom);
    if (elsewhere) {
      store.set(offerErrorAtom, elsewhereRefusal(elsewhere));
      reveal();
      return;
    }
    const running = store.get(sessionAtom);
    if (isActiveSession(running) && !confirmEndFirst(store)) return;
    const controller = new AbortController();
    request = controller;
    try {
      if (isActiveSession(running))
        await endCurrentChat(store, running, controller.signal, "choose the offer");
      if (controller.signal.aborted) return;
      await startChat(store, offer, text, undefined, ownerFeature, plan);
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
