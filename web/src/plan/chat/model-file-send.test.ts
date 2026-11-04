import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Attachment, type Item } from "@/plan/api/schema-chat";

import {
  applyChatFrame,
  composerUnconfirmedAtom,
  registerPending,
  rejectAllPending,
  unconfirmedSendsAtom,
} from "./model";

const fileChip: Attachment = {
  kind: "file",
  path: "",
  anchor: null,
  lines: null,
  stage: null,
  summary_sentence: null,
  rests_on: [],
  quote: "",
  changed_since_selected: false,
  locked: null,
  file: { id: "0123456789abcdef", name: "sketch.png", media_type: "image/png", bytes: 1234 },
};
const send = {
  text: "The sketch shows the layout.",
  attachments: [{ kind: "file" as const, id: "0123456789abcdef", name: "sketch.png" }],
};

function seeded() {
  const store = createStore();
  const snapshot = chatSnapshotSchema.parse(snapshotFixture);
  applyChatFrame(store, { frame: "snapshot", ...snapshot });
  return { store, turnId: snapshot.turns[0]!.id };
}

it("a send carrying a file input is tracked, then confirmed by the delivered file chip", async () => {
  const { store, turnId } = seeded();
  const pending = registerPending(store, "send-file", send);
  const lost = expect(pending).rejects.toThrow("Connection lost");

  rejectAllPending(store, "Connection lost", true);
  await lost;
  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(store.get(composerUnconfirmedAtom)).toHaveLength(1);

  const delivered: Item = {
    type: "user",
    id: "u-file",
    text: send.text,
    attachments: [fileChip],
    delivery: "delivered",
    origin: null,
  };
  applyChatFrame(store, { frame: "delta", op: "item_added", turn_id: turnId, item: delivered });

  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("the same words without the file are not mistaken for the file send", async () => {
  const { store, turnId } = seeded();
  const pending = registerPending(store, "send-file", send);
  const lost = expect(pending).rejects.toThrow("Connection lost");
  rejectAllPending(store, "Connection lost", true);
  await lost;

  const plain: Item = {
    type: "user",
    id: "u-plain",
    text: send.text,
    attachments: [],
    delivery: "delivered",
    origin: null,
  };
  applyChatFrame(store, { frame: "delta", op: "item_added", turn_id: turnId, item: plain });

  expect(store.get(unconfirmedSendsAtom)).toHaveLength(1);
});
