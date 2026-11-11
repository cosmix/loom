import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import {
  chatSnapshotSchema,
  type ChatSnapshot,
  type Item,
  type Turn,
} from "@/plan/api/schema-chat";
import { connectionAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import {
  applyChatFrame,
  CONNECTION_LOST,
  installChatSubscription,
  markInterruptPending,
  queueReturnedAtom,
  registerPending,
  rejectAllPending,
  rejectPending,
  unconfirmedSendsAtom,
} from "./model";

const initial = (): ChatSnapshot => chatSnapshotSchema.parse(snapshotFixture);
const user = (text = "New message"): Extract<Item, { type: "user" }> => ({
  type: "user",
  id: "new-user",
  text,
  attachments: [],
  delivery: "sent",
  origin: null,
  command: null,
});
const turn = (items: Item[] = []): Turn => ({ ...initial().turns[0], id: "new-turn", items });
const seed = (store: Store, snapshot = initial()): void =>
  applyChatFrame(store, { frame: "snapshot", ...snapshot });
const queued = (text: string) => ({ id: `queue-${text}`, first_line: text, text });

it("result resolves the pending command by client_id", async () => {
  const store = createStore();
  const first = registerPending(store, "first");
  const second = registerPending(store, "second");
  const result = { frame: "result", client_id: "second", ok: true, error: null } as const;

  applyChatFrame(store, result);

  await expect(second).resolves.toMatchObject(result);
  const rejected = expect(first).rejects.toThrow("Still pending");
  rejectPending(store, "first", "Still pending");
  await rejected;
});

it("a refused result rejects only its matching command", async () => {
  const store = createStore();
  const pending = registerPending(store, "send", { text: "Kept" });
  const rejected = expect(pending).rejects.toThrow("Not the holder");

  applyChatFrame(store, { frame: "result", client_id: "send", ok: false, error: "Not the holder" });

  await rejected;
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("queue_returned feeds the composer refill atom", () => {
  const store = createStore();
  markInterruptPending(store, "stop");

  applyChatFrame(store, { frame: "queue_returned", texts: ["First", "Second"] });

  expect(store.get(queueReturnedAtom)).toEqual(["First", "Second"]);
});

it("queue_returned ignored without a pending interrupt", () => {
  const store = createStore();

  applyChatFrame(store, { frame: "queue_returned", texts: ["Another tab's text"] });

  expect(store.get(queueReturnedAtom)).toEqual([]);
});

it("queue_returned is accepted after a successful interrupt result only once", async () => {
  const store = createStore();
  const pending = registerPending(store, "stop");
  markInterruptPending(store, "stop");
  applyChatFrame(store, { frame: "result", client_id: "stop", ok: true, error: null });
  await pending;

  applyChatFrame(store, { frame: "queue_returned", texts: ["Returned"] });
  applyChatFrame(store, { frame: "queue_returned", texts: ["Unsolicited"] });

  expect(store.get(queueReturnedAtom)).toEqual(["Returned"]);
});

it("drop before result rejects and keeps the client_id unconfirmed", async () => {
  const store = createStore();
  const dispose = installChatSubscription(store);
  const pending = registerPending(store, "send", { text: "Keep this", attachments: [] });
  const stopped = registerPending(store, "stop");
  markInterruptPending(store, "stop");
  const rejected = expect(pending).rejects.toThrow(CONNECTION_LOST);
  const stopRejected = expect(stopped).rejects.toThrow(CONNECTION_LOST);

  store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null });
  applyChatFrame(store, { frame: "queue_returned", texts: ["Uncertain"] });

  await Promise.all([rejected, stopRejected]);
  expect(store.get(unconfirmedSendsAtom)).toEqual([
    { client_id: "send", text: "Keep this", attachments: [] },
  ]);
  expect(store.get(queueReturnedAtom)).toEqual([]);
  dispose();
});

it("snapshot confirms an unconfirmed send", () => {
  const store = createStore();
  store.set(unconfirmedSendsAtom, [
    { client_id: "sent", text: "Delivered" },
    { client_id: "missing", text: "Missing" },
  ]);

  seed(store, { ...initial(), turns: [turn([user("Delivered")])] });

  expect(store.get(unconfirmedSendsAtom)).toEqual([{ client_id: "missing", text: "Missing" }]);
});

it("pending commands and interrupt intent are isolated between stores", async () => {
  const first = createStore();
  const second = createStore();
  const pending = registerPending(first, "same-id");
  const rejected = expect(pending).rejects.toThrow("Disposed");
  markInterruptPending(first, "same-id");

  applyChatFrame(second, { frame: "result", client_id: "same-id", ok: true, error: null });
  applyChatFrame(second, { frame: "queue_returned", texts: ["Private"] });
  rejectAllPending(first, "Disposed");

  await rejected;
  expect(second.get(queueReturnedAtom)).toEqual([]);
});

it("assistant text does not confirm an unconfirmed send", () => {
  const store = createStore();
  const sends = [{ client_id: "send", text: "Unconfirmed" }];
  store.set(unconfirmedSendsAtom, sends);

  seed(store, {
    session: null,
    previous: null,
    turns: [turn([{ type: "text", id: "answer", markdown: "Unconfirmed" }])],
    queue: [],
  });

  expect(store.get(unconfirmedSendsAtom)).toEqual(sends);
});

it("a snapshot listing the send in the queue confirms it", () => {
  const store = createStore();
  store.set(unconfirmedSendsAtom, [
    { client_id: "queued", text: "Still queued" },
    { client_id: "missing", text: "Missing" },
  ]);

  seed(store, { ...initial(), turns: [turn()], queue: [queued("Still queued")] });

  expect(store.get(unconfirmedSendsAtom)).toEqual([{ client_id: "missing", text: "Missing" }]);
});

it("a snapshot listing the send in the previous chat confirms it", () => {
  const store = createStore();
  store.set(unconfirmedSendsAtom, [{ client_id: "old", text: "Earlier" }]);

  seed(store, {
    session: null,
    turns: [],
    queue: [],
    previous: { session: initial().session!, turns: [turn([user("Earlier")])] },
  });

  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

/** The previous chat holds "yes"; the new chat's start (or send) of "yes" is lost in transit. */
async function loseNextToPrevious(store: Store): Promise<void> {
  const previous = { session: initial().session!, turns: [turn([{ ...user("yes"), id: "old" }])] };
  seed(store, { session: null, turns: [], queue: [], previous });
  await lose(store, "again", "yes");
}

it("a start delivered while the connection dropped is confirmed once the previous chat is cleared", async () => {
  const store = createStore();
  await loseNextToPrevious(store);
  expect(store.get(unconfirmedSendsAtom)).toEqual([
    { client_id: "again", text: "yes", knownPrevious: 1 },
  ]);

  // The server cleared `previous` when the new chat started; the reconnect snapshot lists one "yes".
  seed(store, { ...initial(), turns: [turn([{ ...user("yes"), id: "new-yes" }])], previous: null });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a start that never arrived stays unconfirmed while the previous chat still shows", async () => {
  const store = createStore();
  await loseNextToPrevious(store);

  seed(store, {
    session: null,
    turns: [],
    queue: [],
    previous: { session: initial().session!, turns: [turn([{ ...user("yes"), id: "old" }])] },
  });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
});

it("a queue delta confirms a send that sits in the queue", () => {
  const store = createStore();
  store.set(unconfirmedSendsAtom, [
    { client_id: "queued", text: "Queued after reconnect" },
    { client_id: "missing", text: "Missing" },
  ]);

  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queued("Queued after reconnect")] });

  expect(store.get(unconfirmedSendsAtom)).toEqual([{ client_id: "missing", text: "Missing" }]);
});

it("a user item delta confirms a send, an assistant item does not", () => {
  const store = createStore();
  seed(store);
  store.set(unconfirmedSendsAtom, [{ client_id: "send", text: "Delivered late" }]);

  applyChatFrame(store, {
    frame: "delta",
    op: "item_added",
    turn_id: "t1",
    item: { type: "text", id: "answer", markdown: "Delivered late" },
  });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  applyChatFrame(store, {
    frame: "delta",
    op: "item_added",
    turn_id: "t1",
    item: user("Delivered late"),
  });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a send queued by a working session survives the drop and is not offered for resend", async () => {
  const store = createStore();
  const dispose = installChatSubscription(store);
  seed(store);
  const pending = registerPending(store, "send", { text: "Queued while working" });
  const rejected = expect(pending).rejects.toThrow(CONNECTION_LOST);

  store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null });
  await rejected;
  expect(store.get(unconfirmedSendsAtom)).toEqual([
    { client_id: "send", text: "Queued while working" },
  ]);

  seed(store, { ...initial(), queue: [queued("Queued while working")] });

  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
  dispose();
});

/** Issues a send, then loses the connection before its result: the send is left unconfirmed. */
async function lose(store: Store, clientId: string, text: string): Promise<void> {
  const pending = registerPending(store, clientId, { text });
  const rejected = expect(pending).rejects.toThrow(CONNECTION_LOST);
  rejectAllPending(store, CONNECTION_LOST, true);
  await rejected;
}

it("an earlier identical message does not confirm a lost later one", async () => {
  const store = createStore();
  seed(store, { ...initial(), turns: [turn([{ ...user("yes"), id: "first-yes" }])] });
  await lose(store, "again", "yes");
  expect(store.get(unconfirmedSendsAtom)).toEqual([{ client_id: "again", text: "yes", known: 1 }]);

  seed(store, { ...initial(), turns: [turn([{ ...user("yes"), id: "first-yes" }])] });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  applyChatFrame(store, {
    frame: "delta",
    op: "item_added",
    turn_id: "new-turn",
    item: { ...user("yes"), id: "second-yes" },
  });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a queued copy confirms a repeated message, an undelivered one does not", async () => {
  const store = createStore();
  const first = { ...user("yes"), id: "first-yes" };
  seed(store, { ...initial(), turns: [turn([first])] });
  await lose(store, "again", "yes");

  seed(store, {
    ...initial(),
    turns: [turn([first, { ...user("yes"), id: "second-yes", delivery: "undelivered" }])],
  });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queued("yes")] });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("an item marked undelivered after its send was accepted leaves no unconfirmed entry", () => {
  const store = createStore();
  seed(store, { ...initial(), turns: [turn([user("Gone")])] });
  applyChatFrame(store, {
    frame: "delta",
    op: "item_replaced",
    turn_id: "new-turn",
    item: { ...user("Gone"), delivery: "undelivered" },
  });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a resend lost again replaces the earlier entry for the same message", async () => {
  const store = createStore();
  seed(store);
  await lose(store, "first", "Resend me");
  await lose(store, "second", "Resend me");
  await lose(store, "other", "Different");

  expect(store.get(unconfirmedSendsAtom)).toEqual([
    { client_id: "second", text: "Resend me" },
    { client_id: "other", text: "Different" },
  ]);
});
