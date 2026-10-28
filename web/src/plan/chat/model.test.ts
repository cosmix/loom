import { createStore } from "jotai/vanilla";
import { expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import deltasFixture from "@/plan/api/fixtures/chat-deltas.json";
import {
  chatDeltaSchema,
  chatSnapshotSchema,
  type ChatDelta,
  type ChatSnapshot,
  type Item,
  type Turn,
} from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import {
  applyChatFrame,
  installChatSubscription,
  pendingPermissionsAtom,
  queueAtom,
  sessionAtom,
  snapshotAtom,
  turnsAtom,
} from "./model";

const initial = (): ChatSnapshot => chatSnapshotSchema.parse(snapshotFixture);
const user = (text = "New message"): Item => ({
  type: "user",
  id: "new-user",
  text,
  attachments: [],
  delivery: "sent",
  origin: null,
});
const turn = (items: Item[] = []): Turn => ({ ...initial().turns[0], id: "new-turn", items });
const seed = (store: Store, snapshot = initial()): void =>
  applyChatFrame(store, { frame: "snapshot", ...snapshot });
function delta<Op extends ChatDelta["op"]>(op: Op): Extract<ChatDelta, { op: Op }> {
  return chatDeltaSchema.parse(deltasFixture.find((entry) => entry.op === op)) as Extract<
    ChatDelta,
    { op: Op }
  >;
}

function socketHarness(store: Store) {
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const resync = new Set<() => void>();
  const handle = {
    send: vi.fn(() => true),
    dispose: vi.fn(),
    subscribe: (type: string, handler: (message: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    onResync: (handler: () => void) => {
      resync.add(handler);
      return () => {
        resync.delete(handler);
      };
    },
  };
  store.set(planSocketAtom, handle as unknown as ReturnType<typeof connectPlanSocket>);
  return {
    receive: (message: unknown) => listeners.get("chat")?.forEach((handler) => handler(message)),
    resync: () => resync.forEach((handler) => handler()),
  };
}

it("snapshot replaces state", () => {
  const store = createStore();
  seed(store);
  const replacement: ChatSnapshot = {
    session: null,
    turns: [],
    queue: [],
    previous: initial().previous,
  };

  seed(store, replacement);

  expect(store.get(snapshotAtom)).toEqual(replacement);
});

it("session", () => {
  const store = createStore();
  seed(store);
  const change = delta("session");

  applyChatFrame(store, { frame: "delta", ...change });

  expect(store.get(snapshotAtom)).toEqual({ ...initial(), session: change.session });
  expect(store.get(sessionAtom)).toEqual(change.session);
});

it("turn_added", () => {
  const store = createStore();
  seed(store);
  const added = turn();

  applyChatFrame(store, { frame: "delta", op: "turn_added", turn: added });

  expect(store.get(turnsAtom)).toEqual([...initial().turns, added]);
});

it("turn_updated", () => {
  const store = createStore();
  seed(store);
  const { op: _op, turn_id, ...fields } = delta("turn_updated");

  applyChatFrame(store, { frame: "delta", op: "turn_updated", turn_id, ...fields });

  expect(store.get(turnsAtom)[0]).toEqual({ ...initial().turns[0], ...fields });
});

it("item_added", () => {
  const store = createStore();
  seed(store);
  const item = user();

  applyChatFrame(store, { frame: "delta", op: "item_added", turn_id: "t1", item });

  expect(store.get(turnsAtom)[0].items).toEqual([...initial().turns[0].items, item]);
});

it("item_replaced", () => {
  const store = createStore();
  const item = user();
  seed(store, {
    ...initial(),
    turns: [turn([item, { type: "text", id: "answer", markdown: "Kept" }])],
  });
  const replacement = { ...item, text: "Replaced" };

  applyChatFrame(store, {
    frame: "delta",
    op: "item_replaced",
    turn_id: "new-turn",
    item: replacement,
  });

  expect(store.get(turnsAtom)[0].items).toEqual([
    replacement,
    { type: "text", id: "answer", markdown: "Kept" },
  ]);
});

it("item_removed", () => {
  const store = createStore();
  seed(store, { ...initial(), turns: [turn([user(), { ...user("Kept"), id: "other" }])] });

  applyChatFrame(store, {
    frame: "delta",
    op: "item_removed",
    turn_id: "new-turn",
    id: "new-user",
  });

  expect(store.get(turnsAtom)[0].items).toEqual([{ ...user("Kept"), id: "other" }]);
});

it("queue", () => {
  const store = createStore();
  const { queue } = delta("queue");

  applyChatFrame(store, { frame: "delta", op: "queue", queue });

  expect(store.get(queueAtom)).toEqual(queue);
});

it("cleared", () => {
  const store = createStore();
  seed(store);

  applyChatFrame(store, { frame: "delta", op: "cleared" });

  expect(store.get(snapshotAtom)).toEqual({
    session: null,
    turns: [],
    queue: [],
    previous: { session: initial().session, turns: initial().turns },
  });
});

it("previous kept until first item", () => {
  const store = createStore();
  seed(store);
  applyChatFrame(store, { frame: "delta", op: "cleared" });
  applyChatFrame(store, {
    frame: "delta",
    op: "session",
    session: { ...initial().session!, id: "next" },
  });
  applyChatFrame(store, { frame: "delta", op: "turn_added", turn: turn() });
  expect(store.get(snapshotAtom).previous).toEqual({
    session: initial().session,
    turns: initial().turns,
  });

  applyChatFrame(store, { frame: "delta", op: "item_added", turn_id: "new-turn", item: user() });

  expect(store.get(snapshotAtom).previous).toBeNull();
});

it("cleared then a new session keeps the old chat as previous", () => {
  const store = createStore();
  seed(store);

  applyChatFrame(store, { frame: "delta", op: "cleared" });
  applyChatFrame(store, {
    frame: "delta",
    op: "session",
    session: { ...initial().session!, id: "next" },
  });

  expect(store.get(snapshotAtom)).toEqual({
    session: { ...initial().session!, id: "next" },
    turns: [],
    queue: [],
    previous: { session: initial().session, turns: initial().turns },
  });
});

it("a turn arriving with its first item removes previous", () => {
  const store = createStore();
  seed(store, { ...initial(), turns: [] });

  applyChatFrame(store, { frame: "delta", op: "turn_added", turn: turn([user()]) });

  expect(store.get(snapshotAtom).previous).toBeNull();
});

it("two pending permissions oldest first", () => {
  const store = createStore();
  const permission: Extract<Item, { type: "permission" }> = {
    type: "permission",
    id: "old",
    request_id: "old",
    tool: "Bash",
    input: "{}",
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2026-10-28T12:00:00Z",
  };
  seed(store, {
    ...initial(),
    turns: [
      { ...turn([{ ...permission, id: "new" }]), started_at: "2026-10-28T11:00:00Z" },
      {
        ...turn([permission, { ...permission, id: "answered", status: "allowed" }]),
        started_at: "2026-10-28T10:00:00Z",
      },
    ],
  });

  expect(store.get(pendingPermissionsAtom).map((item) => item.id)).toEqual(["old", "new"]);
});

it("subscriptions follow socket replacement and preserve state until the resync snapshot", () => {
  const store = createStore();
  const first = socketHarness(store);
  const dispose = installChatSubscription(store);
  first.receive({ type: "chat", frame: "snapshot", ...initial() });
  const second = socketHarness(store);
  first.receive({ type: "chat", frame: "delta", op: "cleared" });
  second.resync();
  expect(store.get(snapshotAtom)).toEqual(initial());

  second.receive({
    type: "chat",
    frame: "snapshot",
    session: null,
    turns: [],
    queue: [],
    previous: null,
  });

  expect(store.get(snapshotAtom)).toEqual({ session: null, turns: [], queue: [], previous: null });
  dispose();
  second.receive({ type: "chat", frame: "snapshot", ...initial() });
  expect(store.get(sessionAtom)).toBeNull();
});
