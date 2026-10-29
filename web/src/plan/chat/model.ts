import { atom } from "jotai/vanilla";
import { useEffect } from "react";

import type {
  AttachmentInput,
  ChatDelta,
  ChatFrame,
  ChatSnapshot,
  CommandResult,
  Item,
  Turn,
} from "@/plan/api/schema-chat";
import { connectionAtom, planSocketAtom } from "@/plan/state/atoms";
import {
  isQueued,
  messageIdentity,
  queuedIds,
  sameMessage,
  type MessageContent,
} from "./model-identity";
import type { Store } from "@/state/store";

const emptySnapshot = (): ChatSnapshot => ({ session: null, turns: [], queue: [], previous: null });
export const snapshotAtom = atom<ChatSnapshot>(emptySnapshot());
export const sessionAtom = atom((get) => get(snapshotAtom).session);
export const turnsAtom = atom((get) => get(snapshotAtom).turns);
export const queueAtom = atom((get) => get(snapshotAtom).queue);
export const pendingPermissionsAtom = atom((get) =>
  [...get(turnsAtom)]
    .sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at))
    .flatMap((turn) => turn.items)
    .filter(
      (item): item is Extract<Item, { type: "permission" }> =>
        item.type === "permission" && item.status === "pending",
    ),
);
export const queueReturnedAtom = atom<string[]>([]);

export type PendingSend = { text: string; attachments?: AttachmentInput[] };
/**
 * `known` is how many identical messages the current chat's transcript and queue already showed
 * when the send was issued, `knownPrevious` how many the previous chat showed (absent means
 * none). The send counts as delivered once more than that are shown in either place. They are
 * tracked apart because the server clears the previous chat when a new one starts.
 * A queue entry carries text only, so a send with attachments cannot be matched to it by
 * identity: `knownQueuedIds` are the ids of the queued messages that had the send's text when it
 * was issued, and the send is queued while a text-matching entry outside that set waits. `queued`
 * records that it was seen queued, so its leaving the queue unmatched (withdrawn, or returned to
 * the composer) drops the entry.
 */
export type UnconfirmedSend = PendingSend & {
  client_id: string;
  known?: number;
  knownPrevious?: number;
  knownQueuedIds?: string[];
  queued?: boolean;
};
export const unconfirmedSendsAtom = atom<UnconfirmedSend[]>([]);

/** Identifies a user item across chats: turn ids repeat per session, start times do not. */
export const resendKey = (turn: Turn, itemId: string) => `${turn.id}|${turn.started_at}|${itemId}`;
/**
 * Undelivered items whose Resend was issued and not refused at once; in memory, so a remount does
 * not bring the control back. After a connection loss the unconfirmed entry stands for the message.
 */
export const resentItemsAtom = atom<ReadonlySet<string>>(new Set<string>());
/** Client ids of unconfirmed entries a transcript Resend is sending right now. */
export const resendingEntriesAtom = atom<ReadonlySet<string>>(new Set<string>());

/**
 * Unconfirmed entries the composer lists: those whose message neither carries its own transcript
 * Resend (an undelivered item of the current chat not yet resent) nor is being resent by one, so
 * a lost message has exactly one control. A message waiting in the queue strip is listed there
 * only.
 */
export const composerUnconfirmedAtom = atom((get) => {
  const resent = get(resentItemsAtom);
  const resending = get(resendingEntriesAtom);
  const queue = get(queueAtom);
  const carried: MessageContent[] = [];
  for (const turn of get(turnsAtom)) {
    for (const item of turn.items) {
      if (item.type !== "user" || item.delivery !== "undelivered" || item.origin !== null) continue;
      if (!resent.has(resendKey(turn, item.id))) carried.push(item);
    }
  }
  return get(unconfirmedSendsAtom).filter(
    (send) =>
      !resending.has(send.client_id) &&
      !isQueued(send, queue) &&
      !carried.some((message) => sameMessage(message, send)),
  );
});
export const RESEND_OFFLINE = "Reconnect to check delivery before resending";
export const CONNECTION_LOST = "Connection lost; check the transcript before resending";

type Pending = {
  resolve: (result: CommandResult) => void;
  reject: (reason: Error) => void;
  send?: PendingSend;
  known: Known;
};
type Known = { current: number; previous: number; queued: string[] };
const pendingCommands = new WeakMap<Store, Map<string, Pending>>();
const pendingInterrupts = new WeakMap<Store, Set<string>>();

function commandsFor(store: Store): Map<string, Pending> {
  let commands = pendingCommands.get(store);
  if (!commands) pendingCommands.set(store, (commands = new Map()));
  return commands;
}

/** Register before socket.send; provide send content only for start/send messages. */
export function registerPending(
  store: Store,
  clientId: string,
  send?: PendingSend,
): Promise<CommandResult> {
  const commands = commandsFor(store);
  if (commands.has(clientId)) throw new Error(`Command already pending: ${clientId}`);
  const none = { current: 0, previous: 0, queued: [] };
  const known = send ? knownCopies(store.get(snapshotAtom), send) : none;
  return new Promise((resolve, reject) => commands.set(clientId, { resolve, reject, send, known }));
}

/** Marks a command whose result the server may follow with `queue_returned` (interrupt, end). */
export function markInterruptPending(store: Store, clientId: string): void {
  let interrupts = pendingInterrupts.get(store);
  if (!interrupts) pendingInterrupts.set(store, (interrupts = new Set()));
  interrupts.add(clientId);
}

export function rejectPending(store: Store, clientId: string, reason: string | Error): void {
  const commands = commandsFor(store);
  const pending = commands.get(clientId);
  commands.delete(clientId);
  pendingInterrupts.get(store)?.delete(clientId);
  pending?.reject(reason instanceof Error ? reason : new Error(reason));
}

/** A resend that is lost again replaces the earlier entry for the same message: one Resend each. */
function markUnconfirmed(store: Store, clientId: string, send: PendingSend, known: Known): void {
  const entry: UnconfirmedSend = {
    ...send,
    client_id: clientId,
    ...(known.current > 0 ? { known: known.current } : {}),
    ...(known.previous > 0 ? { knownPrevious: known.previous } : {}),
    ...(known.queued.length > 0 ? { knownQueuedIds: known.queued } : {}),
  };
  store.set(unconfirmedSendsAtom, (sends) => [
    ...sends.filter((other) => other.client_id !== clientId && !sameMessage(other, entry)),
    entry,
  ]);
}

/** Only a connection loss makes sent content uncertain; immediate refusals do not. */
export function rejectAllPending(store: Store, reason: string | Error, unconfirmed = false): void {
  for (const [clientId, pending] of commandsFor(store)) {
    if (unconfirmed && pending.send) markUnconfirmed(store, clientId, pending.send, pending.known);
    rejectPending(store, clientId, reason);
  }
  pendingInterrupts.delete(store);
}

export function resolvePending(store: Store, result: CommandResult): void {
  if (result.client_id === null) return;
  if (!result.ok) {
    rejectPending(store, result.client_id, result.error ?? "Command refused");
    return;
  }
  const commands = commandsFor(store);
  const pending = commands.get(result.client_id);
  commands.delete(result.client_id);
  // dispatch.rs sends result before queue_returned. Keep the interrupt intent until refill.
  pending?.resolve(result);
}

function previousChat(state: ChatSnapshot): ChatSnapshot["previous"] {
  return state.session && state.turns.length
    ? { session: state.session, turns: state.turns }
    : state.previous;
}

function updateTurn(state: ChatSnapshot, id: string, update: (turn: Turn) => Turn): ChatSnapshot {
  return { ...state, turns: state.turns.map((turn) => (turn.id === id ? update(turn) : turn)) };
}

function applyDelta(state: ChatSnapshot, delta: ChatDelta): ChatSnapshot {
  switch (delta.op) {
    case "session":
      // The bridge sends `cleared` before a new session's `session` (start.rs), so only replace it.
      return { ...state, session: delta.session };
    case "turn_added":
      return {
        ...state,
        turns: [...state.turns, delta.turn],
        previous: delta.turn.items.length ? null : state.previous,
      };
    case "turn_updated": {
      const {
        frame: _frame,
        op: _op,
        turn_id,
        ...fields
      } = delta as typeof delta & { frame?: string };
      return updateTurn(state, turn_id, (turn) => ({ ...turn, ...fields }));
    }
    case "item_added": {
      const next = updateTurn(state, delta.turn_id, (turn) => ({
        ...turn,
        items: [...turn.items, delta.item],
      }));
      return state.turns.some((turn) => turn.id === delta.turn_id)
        ? { ...next, previous: null }
        : next;
    }
    case "item_replaced":
      return updateTurn(state, delta.turn_id, (turn) => ({
        ...turn,
        items: turn.items.map((item) => (item.id === delta.item.id ? delta.item : item)),
      }));
    case "item_removed":
      return updateTurn(state, delta.turn_id, (turn) => ({
        ...turn,
        items: turn.items.filter((item) => item.id !== delta.id),
      }));
    case "queue":
      return { ...state, queue: delta.queue };
    case "cleared":
      return { ...emptySnapshot(), previous: previousChat(state) };
  }
}

/**
 * How often each message identity is shown as a delivered-or-pending user item or, for the current
 * chat, a queued message. An `undelivered` item is the bridge saying the message did not arrive,
 * so it never counts. A queued entry carries text only, so it has the identity of a message
 * without attachments and never counts toward one that has attachments (`isQueued` covers those).
 */
function shownCounts(turns: Turn[], queue: ChatSnapshot["queue"] = []): Map<string, number> {
  const counts = new Map<string, number>();
  const add = (message: MessageContent) => {
    const key = messageIdentity(message);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  };
  for (const item of turns.flatMap((turn) => turn.items)) {
    if (item.type === "user" && item.delivery !== "undelivered") add(item);
  }
  for (const queued of queue) if (queued.text !== null) add({ text: queued.text });
  return counts;
}

function knownCopies(snapshot: ChatSnapshot, send: MessageContent): Known {
  const key = messageIdentity(send);
  return {
    current: shownCounts(snapshot.turns, snapshot.queue).get(key) ?? 0,
    previous: shownCounts(snapshot.previous?.turns ?? []).get(key) ?? 0,
    queued: queuedIds(snapshot.queue, send.text),
  };
}

/**
 * A send is confirmed once more copies of its message are shown than before it was issued. One
 * that waits in the queue is marked `queued`; one marked so that has left the queue without a
 * matching item was withdrawn or returned to the composer, so it is dropped (a delivered item
 * confirms it above, an undelivered one carries its own Resend). The holder's tab receives each
 * queue delta first with the texts hidden, then with them (plan_server/chat/mod.rs `publish`), so
 * a queued entry also drops on a later queue change while its message is still queued. That is
 * harmless: the server keeps the message, which is delivered or withdrawn there regardless.
 */
function confirmSends(store: Store): void {
  if (!store.get(unconfirmedSendsAtom).length) return;
  const snapshot = store.get(snapshotAtom);
  const current = shownCounts(snapshot.turns, snapshot.queue);
  const previous = shownCounts(snapshot.previous?.turns ?? []);
  const settle = (send: UnconfirmedSend): UnconfirmedSend[] => {
    const key = messageIdentity(send);
    if (
      (current.get(key) ?? 0) > (send.known ?? 0) ||
      (previous.get(key) ?? 0) > (send.knownPrevious ?? 0)
    ) {
      return [];
    }
    if (isQueued(send, snapshot.queue)) return [send.queued ? send : { ...send, queued: true }];
    return send.queued ? [] : [send];
  };
  store.set(unconfirmedSendsAtom, (sends) => {
    const next = sends.flatMap(settle);
    return next.length === sends.length && next.every((send, i) => send === sends[i])
      ? sends
      : next;
  });
}

export function applyChatFrame(store: Store, frame: ChatFrame): void {
  switch (frame.frame) {
    case "snapshot": {
      const { session, turns, queue, previous } = frame;
      const snapshot = { session, turns, queue, previous };
      store.set(snapshotAtom, snapshot);
      confirmSends(store);
      break;
    }
    case "delta":
      store.set(snapshotAtom, (state) => applyDelta(state, frame));
      confirmSends(store);
      break;
    case "result":
      resolvePending(store, frame);
      break;
    case "queue_returned": {
      const interrupts = pendingInterrupts.get(store);
      if (!interrupts?.size) break;
      interrupts.delete(interrupts.values().next().value!);
      store.set(queueReturnedAtom, frame.texts);
      break;
    }
  }
}

/** Install once per store; follows handle replacement as well as reconnects. */
export function installChatSubscription(store: Store): () => void {
  let detach = () => {};
  const attach = () => {
    detach();
    const socket = store.get(planSocketAtom);
    if (!socket) return;
    // socket.ts has already validated the frame; only the `type` tag is stripped.
    const unsubscribe = socket.subscribe("chat", ({ type: _type, ...frame }) => {
      applyChatFrame(store, frame);
    });
    const stopResync = socket.onResync(() => {
      // Plan 7 on_open supplies the authoritative snapshot. Keep the visible transcript meanwhile.
      pendingInterrupts.delete(store);
    });
    detach = () => {
      unsubscribe();
      stopResync();
    };
  };
  attach();
  const stopSocket = store.sub(planSocketAtom, attach);
  const stopConnection = store.sub(connectionAtom, () => {
    const { phase } = store.get(connectionAtom);
    if (phase === "reconnecting" || phase === "unauthorized") {
      rejectAllPending(store, CONNECTION_LOST, true);
    }
  });
  return () => {
    detach();
    stopSocket();
    stopConnection();
  };
}

export function useChatSocket(store: Store): void {
  useEffect(() => installChatSubscription(store), [store]);
}
