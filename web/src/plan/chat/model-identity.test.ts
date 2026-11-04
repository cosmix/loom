import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import {
  chatSnapshotSchema,
  type Attachment,
  type AttachmentInput,
  type Item,
} from "@/plan/api/schema-chat";
import type { Store } from "@/state/store";

import {
  applyChatFrame,
  composerUnconfirmedAtom,
  CONNECTION_LOST,
  markInterruptPending,
  queueReturnedAtom,
  registerPending,
  rejectAllPending,
  unconfirmedSendsAtom,
} from "./model";
import { sameMessage } from "./model-identity";
import { toWire } from "./wire-input-test-kit";

const initial = chatSnapshotSchema.parse(snapshotFixture);
type UserItem = Extract<Item, { type: "user" }>;

const attached = (fields: Partial<Attachment> = {}): Attachment => ({
  kind: "source",
  path: "doc/x.md",
  anchor: null,
  lines: [1, 2],
  stage: null,
  summary_sentence: null,
  rests_on: [],
  quote: "Quote",
  changed_since_selected: false,
  locked: null,
  ...fields,
});
const wire = (chip: Attachment) => toWire(chip);
const user = (fields: Partial<UserItem> = {}): UserItem => ({
  type: "user",
  id: "u",
  text: "yes",
  attachments: [],
  delivery: "sent",
  origin: null,
  ...fields,
});

const queuedYes = { id: "q", first_line: "yes", text: "yes" };

function seed(store: Store, items: Item[]): void {
  const turn = { ...initial.turns[0], items };
  const snapshot = { ...initial, turns: [turn], queue: [], previous: null };
  applyChatFrame(store, { frame: "snapshot", ...snapshot });
}
function deliver(store: Store, item: UserItem): void {
  applyChatFrame(store, {
    frame: "delta",
    op: "item_added",
    turn_id: initial.turns[0].id,
    item,
  });
}
function lose(store: Store, text: string, attachments: AttachmentInput[] = []): void {
  registerPending(store, "lost", { text, attachments }).catch(() => {});
  rejectAllPending(store, CONNECTION_LOST, true);
}

it("attachments at different lines or of a different kind are different messages", () => {
  const base = { text: "yes", attachments: [attached()] };
  expect(sameMessage(base, { text: "yes", attachments: [attached()] })).toBe(true);
  expect(sameMessage(base, { text: "yes", attachments: [attached({ lines: [5, 6] })] })).toBe(
    false,
  );
  expect(sameMessage(base, { text: "yes", attachments: [attached({ lines: null })] })).toBe(false);
  expect(sameMessage(base, { text: "yes", attachments: [attached({ kind: "summary" })] })).toBe(
    false,
  );
});

it("a message with control characters is the same message the server delivers sanitised", () => {
  expect(sameMessage({ text: "red \u001b[31mtext\u0007\r\n" }, { text: "red [31mtext\n" })).toBe(
    true,
  );
  expect(sameMessage({ text: "a\tb\nc" }, { text: "ab\nc" })).toBe(false);
});

it("an entry with control characters is confirmed by the server's sanitised item", () => {
  const store = createStore();
  seed(store, []);
  lose(store, "red \u001b[31mtext");
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  deliver(store, user({ text: "red [31mtext" }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("an entry is listed in the composer only while no undelivered item stands for it", () => {
  const store = createStore();
  lose(store, "red \u001b[31mtext");
  seed(store, [user({ text: "red [31mtext", delivery: "undelivered" })]);
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);
});

it("a delivered message with other attachments does not confirm an entry", () => {
  const store = createStore();
  seed(store, []);
  lose(store, "yes", [wire(attached({ path: "doc/y.md" }))]);

  deliver(store, user({ attachments: [attached({ path: "doc/x.md" })] }));
  deliver(store, user({ id: "v", attachments: [attached({ path: "doc/y.md", lines: [8, 9] })] }));
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  deliver(store, user({ id: "w", attachments: [attached({ path: "doc/y.md" })] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a queued message confirms an entry without attachments only", () => {
  const store = createStore();
  seed(store, []);
  lose(store, "yes");
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

// What the transcript's parse_message gives back for a sent attachment (attachments/parse.rs).
const parsed = attached;
const CUT = (n: number) => `[…${n} more characters; read the source]`;

it("a stage attachment is confirmed by the delivered form, whose anchor is stage:<id>", () => {
  const store = createStore();
  seed(store, []);
  const sent = attached({ path: "doc/plan.md", anchor: "browser-anchor", stage: "build" });
  lose(store, "yes", [wire(sent)]);
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);

  deliver(
    store,
    user({
      id: "u-1",
      attachments: [parsed({ path: "doc/plan.md", anchor: "stage:build", stage: "build" })],
    }),
  );
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a summary attachment is confirmed by its sentence id as the anchor", () => {
  const store = createStore();
  seed(store, []);
  // The server refuses a summary attachment whose anchor differs from its sentence (resolve.rs).
  const sent = attached({
    kind: "summary",
    anchor: "s-0123abcd",
    summary_sentence: "s-0123abcd",
    lines: null,
  });
  lose(store, "yes", [wire(sent)]);

  const back = parsed({
    kind: "summary",
    anchor: "s-0123abcd",
    summary_sentence: "s-0123abcd",
    lines: null,
  });
  deliver(store, user({ id: "u-1", attachments: [back] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a quote over 2,000 characters is confirmed by the cut quote the transcript returns", () => {
  const store = createStore();
  seed(store, []);
  const long = "x".repeat(2500);
  lose(store, "yes", [wire(attached({ quote: long }))]);

  const cut = `${"x".repeat(2000)}\n${CUT(500)}`;
  deliver(store, user({ id: "u-1", attachments: [parsed({ quote: cut })] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a quote with control characters is confirmed by the delivered quote without them", () => {
  const store = createStore();
  seed(store, []);
  lose(store, "yes", [wire(attached({ quote: "a\u001b[31mb\u0007\r\nc" }))]);

  deliver(store, user({ id: "u-1", attachments: [parsed({ quote: "a[31mb\nc" })] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("attachments that differ in address are different messages", () => {
  const one = { text: "yes", attachments: [attached({ anchor: "a" })] };
  expect(sameMessage(one, { text: "yes", attachments: [attached({ anchor: "b" })] })).toBe(false);
  expect(sameMessage(one, { text: "yes", attachments: [attached({ stage: "b" })] })).toBe(false);
});

const lostWithAttachment = (store: Store) => {
  seed(store, []);
  lose(store, "yes", [wire(attached())]);
};

it("a lost send the server queued is listed in the queue strip only", () => {
  const store = createStore();
  lostWithAttachment(store);
  expect(store.get(composerUnconfirmedAtom)).toHaveLength(1);

  applyChatFrame(store, { frame: "snapshot", ...initial, turns: [], queue: [queuedYes] });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);
});

it("a queued text that was already waiting when the send was issued does not hide it", () => {
  const store = createStore();
  seed(store, []);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  lose(store, "yes", [wire(attached())]);
  expect(store.get(composerUnconfirmedAtom)).toHaveLength(1);
});

it("withdrawing a queued lost send drops its entry", () => {
  const store = createStore();
  lostWithAttachment(store);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [] });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a queue returned by an interrupt drops the entry and refills the composer", () => {
  const store = createStore();
  lostWithAttachment(store);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  markInterruptPending(store, "stop");
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [] });
  applyChatFrame(store, { frame: "queue_returned", texts: ["yes"] });
  expect(store.get(queueReturnedAtom)).toEqual(["yes"]);
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("the sent item of a queued lost send confirms its entry", () => {
  const store = createStore();
  lostWithAttachment(store);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);

  deliver(store, user({ id: "s-1", attachments: [attached()] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [] });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a lost send that was never queued stays when the queue is empty", () => {
  const store = createStore();
  lostWithAttachment(store);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [] });
  expect(store.get(composerUnconfirmedAtom)).toHaveLength(1);
});

// A rendered-view selection: no lines, the enclosing section heading as anchor, no stage.
const passage = (quote: string) => attached({ anchor: "Scope", lines: null, quote });
const lostAs = (store: Store, clientId: string, text: string, quote: string) =>
  registerPending(store, clientId, { text, attachments: [wire(passage(quote))] }).catch(() => {});

it("two passages of one section with no lines are different messages", () => {
  const one = { text: "rewrite this", attachments: [passage("first passage")] };
  expect(sameMessage(one, { text: "rewrite this", attachments: [passage("second passage")] })).toBe(
    false,
  );
});

it("a later send with another passage does not confirm the lost one", () => {
  const store = createStore();
  seed(store, []);
  lose(store, "rewrite this", [wire(passage("passage 1"))]);

  deliver(store, user({ id: "s-2", text: "rewrite this", attachments: [passage("passage 2")] }));
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(store.get(composerUnconfirmedAtom)).toHaveLength(1);

  deliver(store, user({ id: "s-3", text: "rewrite this", attachments: [passage("passage 1")] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("two in-flight sends with other passages stay two entries", () => {
  const store = createStore();
  seed(store, []);
  void lostAs(store, "a", "rewrite this", "passage 1");
  void lostAs(store, "b", "rewrite this", "passage 2");
  rejectAllPending(store, CONNECTION_LOST, true);
  expect(store.get(unconfirmedSendsAtom).map((send) => send.client_id)).toEqual(["a", "b"]);
});

it("the raw, server-resolved and delivered forms of a quote are one message", () => {
  const sameQuote = (raw: string, delivered: string) => {
    const sent = { text: "yes", attachments: [attached({ quote: raw })] };
    const item = { text: "yes", attachments: [attached({ quote: delivered })] };
    expect(sameMessage(sent, item)).toBe(true);
    expect(sameMessage(item, sent)).toBe(true);
  };
  // 30 lines of 99 characters: 20 fit the 2,000-character cut (1,999), 1,000 characters remain.
  const line = "w".repeat(99);
  const long = Array<string>(30).fill(line).join("\n");
  sameQuote(long, `${Array<string>(20).fill(line).join("\n")}\n${CUT(1000)}`);
  const huge = "x".repeat(2500);
  sameQuote(huge, `${"x".repeat(2000)}\n${CUT(500)}`);
  sameQuote("a\u001b[31mb\u0007\r\nc", "a[31mb\nc");
  expect(
    sameMessage(
      { text: "yes", attachments: [attached({ quote: huge })] },
      { text: "yes", attachments: [attached({ quote: `${"x".repeat(2000)}\n${CUT(499)}` })] },
    ),
  ).toBe(false);
});

it("an earlier copy leaving the queue does not make a queued lost send look unqueued", () => {
  const store = createStore();
  seed(store, []);
  applyChatFrame(store, {
    frame: "delta",
    op: "queue",
    queue: [{ id: "q1", first_line: "continue", text: "continue" }],
  });
  lose(store, "continue", [wire(attached())]);

  // The first 'continue' is delivered during the outage; the server queued the send as q2.
  applyChatFrame(store, {
    frame: "snapshot",
    ...initial,
    turns: [],
    queue: [{ id: "q2", first_line: "continue", text: "continue" }],
  });
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);
});

it("a queued lost send delivered in the server's order leaves no entry behind", () => {
  const store = createStore();
  lostWithAttachment(store);
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [queuedYes] });
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);

  // next_delivery publishes the queue pop before it adds the s- item.
  applyChatFrame(store, { frame: "delta", op: "queue", queue: [] });
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
  deliver(store, user({ id: "s-1", attachments: [attached()] }));
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(store.get(composerUnconfirmedAtom)).toEqual([]);
});
