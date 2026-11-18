import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import commandFixtures from "@/plan/api/fixtures/chat-commands.json";
import reviewFixtures from "@/plan/api/fixtures/review-suggestion.json";
import briefFixture from "@/plan/api/fixtures/session-brief.json";
import { chatCommandSchema } from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { reviewMessageSchema, suggestedPassageSchema } from "@/plan/api/schema-review";
import { connectionAtom, planSocketAtom } from "@/plan/state/atoms";
import {
  answerPermission,
  answerSuggestion,
  endChat,
  getSessionBrief,
  interruptChat,
  sendChatCommand,
  sendMessage,
  startChat,
  suggest,
  toAttachmentInput,
  withdrawMessage,
} from "./commands";
import { draftAtom } from "./drafts";
import {
  CONNECTION_LOST,
  installChatSubscription,
  queueReturnedAtom,
  snapshotAtom,
  unconfirmedSendsAtom,
} from "./model";

const source: AttachmentDraft = {
  kind: "source",
  path: "doc/product/section_10.md",
  anchor: "review",
  text: "Selected source",
  lines: { start: 12, end: 18 },
  stageId: "w1",
  hash: "source-hash",
};
const summary: AttachmentDraft = {
  kind: "summary",
  summary: "product",
  sentence: "Review needs context",
  text: "Selected summary",
  restsOn: [{ path: "doc/product/section_10.md", anchor: "review" }],
  hash: "summary-hash",
};
const passage = suggestedPassageSchema.parse(reviewFixtures[0].passage);

function harness(store = createStore()) {
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const send = vi.fn((_message: unknown) => store.get(connectionAtom).phase === "open");
  const socket = {
    send,
    subscribe: (type: string, handler: (message: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  const receive = (type: string, frame: object) => {
    [...(listeners.get(type) ?? [])].forEach((handler) => handler({ type, ...frame }));
  };
  const result = (index: number, error: string | null = null) => {
    const message = send.mock.calls[index][0] as { client_id: string };
    receive("chat", { frame: "result", client_id: message.client_id, ok: error === null, error });
  };
  return { store, send, receive, result, listeners };
}

beforeEach(() => {
  let sequence = 0;
  vi.spyOn(crypto, "randomUUID").mockImplementation(
    () => `00000000-0000-4000-8000-${String(++sequence).padStart(12, "0")}`,
  );
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});

it("send while closed rejects and keeps the draft", async () => {
  const { store, send } = harness();
  const draft = { text: "Keep this message", attachments: [source] };
  store.set(draftAtom, draft);
  store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null });

  const pending = sendMessage(store, draft.text, draft.attachments.map(toAttachmentInput));

  await expect(pending).rejects.toThrow("Not connected; the message is kept");
  expect(send).toHaveBeenCalledOnce();
  expect(store.get(draftAtom)).toEqual(draft);
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("sending without a socket rejects immediately", async () => {
  await expect(sendMessage(createStore(), "Keep this")).rejects.toThrow(
    "Not connected; the message is kept",
  );
});

it("drop before result rejects and keeps the client_id unconfirmed", async () => {
  const { store, send } = harness();
  const attachments = [toAttachmentInput(summary)];
  const pending = sendMessage(store, "Check the transcript", attachments);
  const interrupt = interruptChat(store);
  const sending = expect(pending).rejects.toThrow(CONNECTION_LOST);
  const stopping = expect(interrupt).rejects.toThrow(CONNECTION_LOST);
  const { client_id } = send.mock.calls[0][0] as { client_id: string };
  store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null });

  await Promise.all([sending, stopping]);
  expect(store.get(unconfirmedSendsAtom)).toEqual([
    { client_id, text: "Check the transcript", attachments },
  ]);
});

it("toAttachmentInput source and summary arms", () => {
  expect(toAttachmentInput(source)).toEqual({
    kind: "source",
    path: "doc/product/section_10.md",
    anchor: "review",
    lines: [12, 18],
    stage: "w1",
    summary_sentence: null,
    rests_on: [],
    quote: "Selected source",
    selected_hash: "source-hash",
  });
  expect(toAttachmentInput(summary)).toEqual({
    kind: "summary",
    path: "doc/loom/summaries/product.md",
    anchor: "Review needs context",
    lines: null,
    stage: null,
    summary_sentence: "Review needs context",
    rests_on: ["doc/product/section_10.md#review"],
    quote: "Selected summary",
    selected_hash: "summary-hash",
  });
  expect(toAttachmentInput({ ...source, lines: null, hash: null })).toMatchObject({
    lines: null,
    selected_hash: null,
  });
});

it.each(commandFixtures)("sends the $command fixture with a fresh client_id", async (fixture) => {
  const { store, send, result } = harness();
  const { client_id: _clientId, ...request } = chatCommandSchema.parse(fixture);

  const pending = sendChatCommand(store, request);

  expect(send).toHaveBeenCalledWith({
    type: "chat",
    ...fixture,
    client_id: "00000000-0000-4000-8000-000000000001",
  });
  result(0);
  await expect(pending).resolves.toEqual({
    client_id: "00000000-0000-4000-8000-000000000001",
    ok: true,
    error: null,
  });
});

it("chat helpers send all command fields and resolve results by client_id", async () => {
  const { store, send, result } = harness();
  const attachments = [toAttachmentInput(source)];
  const pending = [
    startChat(store, "offer", "Start here", attachments),
    sendMessage(store, "Next", attachments),
    withdrawMessage(store, "queue"),
    answerPermission(store, "request", false, "Inspect first"),
    interruptChat(store),
    endChat(store),
  ];
  expect(
    send.mock.calls.map(([message]) => {
      const { client_id, ...fields } = message as Record<string, unknown>;
      expect(client_id).toEqual(expect.any(String));
      return fields;
    }),
  ).toEqual([
    { type: "chat", command: "start", offer: "offer", text: "Start here", attachments },
    { type: "chat", command: "send", text: "Next", attachments },
    { type: "chat", command: "withdraw", queue_id: "queue" },
    {
      type: "chat",
      command: "permission",
      request_id: "request",
      allow: false,
      note: "Inspect first",
    },
    { type: "chat", command: "interrupt" },
    { type: "chat", command: "end" },
  ]);
  result(5);
  result(3);
  result(1);
  result(4);
  result(0);
  result(2);
  const accepted = await Promise.all(pending);
  expect(new Set(accepted.map((result) => result.client_id)).size).toBe(6);
});

it("start permits an offer without text or attachments", async () => {
  const { store, send, result } = harness();
  const pending = startChat(store, "offer");
  expect(send.mock.calls[0][0]).toMatchObject({ offer: "offer", text: null, attachments: [] });
  expect(send.mock.calls[0][0]).not.toHaveProperty("feature");
  result(0);
  await pending;
});

it("start includes the feature alongside the existing positional fields", async () => {
  const { store, send, result } = harness();
  const attachments = [toAttachmentInput(source)];

  const pending = startChat(store, "write-section:02", "Start here", attachments, "demo");

  expect(send).toHaveBeenCalledWith({
    type: "chat",
    command: "start",
    client_id: expect.any(String),
    offer: "write-section:02",
    feature: "demo",
    text: "Start here",
    attachments,
  });
  result(0);
  await pending;
});

it("start names a standalone plan, and sends no owner key it was not given", async () => {
  const { store, send, result } = harness();

  const pending = startChat(store, null, null, [], undefined, "solo");

  expect(send.mock.calls[0]![0]).toMatchObject({ command: "start", plan: "solo" });
  expect(send.mock.calls[0]![0]).not.toHaveProperty("feature");
  result(0);
  await pending;
});

it("a server refusal rejects only the matching command and keeps the draft", async () => {
  const { store, receive, result } = harness();
  const draft = { text: "Retain this", attachments: [source] };
  store.set(draftAtom, draft);
  const pending = sendMessage(store, draft.text);
  const other = endChat(store);
  const refused = expect(pending).rejects.toThrow("Only the holder may send");

  receive("refused", { command: "send", reason: "Only the holder may send" });
  result(1);

  await refused;
  await expect(other).resolves.toMatchObject({ ok: true });
  expect(store.get(draftAtom)).toEqual(draft);
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a refused chat result preserves the server reason", async () => {
  const { store, result } = harness();
  const pending = answerPermission(store, "request", true);
  const refused = expect(pending).rejects.toThrow("Permission is no longer pending");
  result(0, "Permission is no longer pending");
  await refused;
});

it("interrupt intent permits queue_returned after its result", async () => {
  const { store, receive, result } = harness();
  const dispose = installChatSubscription(store);
  const pending = interruptChat(store);

  result(0);
  receive("chat", { frame: "queue_returned", texts: ["Returned draft"] });

  await pending;
  expect(store.get(queueReturnedAtom)).toEqual(["Returned draft"]);
  dispose();
});

it("end permits queue_returned after its result, once", async () => {
  const { store, receive, result } = harness();
  const dispose = installChatSubscription(store);
  const pending = endChat(store);

  result(0);
  receive("chat", { frame: "queue_returned", texts: ["Unsent while ending"] });
  receive("chat", { frame: "queue_returned", texts: ["Unsolicited"] });

  await pending;
  expect(store.get(queueReturnedAtom)).toEqual(["Unsent while ending"]);
  dispose();
});

it("suggest resolves on sent status and preserves the review wire shape", async () => {
  const { store, send, receive } = harness();
  const pending = suggest(store, passage, "Review this");
  expect(send).toHaveBeenCalledWith({
    type: "review",
    kind: "suggest",
    passage,
    note: "Review this",
  });

  receive("review", reviewMessageSchema.parse(reviewFixtures[1]));

  await expect(pending).resolves.toMatchObject({ ok: true });
});

it.each(["attach", "dismiss"] as const)(
  "suggestion_answer awaits its %s status",
  async (action) => {
    const { store, send, receive } = harness();
    const pending = answerSuggestion(store, "sg-1", action);
    expect(send).toHaveBeenCalledWith({
      type: "review",
      kind: "suggestion_answer",
      id: "sg-1",
      action,
    });
    receive("review", { kind: "suggestion_status", id: "sg-other", state: "attached" });
    receive("review", {
      kind: "suggestion_status",
      id: "sg-1",
      state: action === "attach" ? "attached" : "dismissed",
    });
    await expect(pending).resolves.toMatchObject({ ok: true });
  },
);

it("review refusal rejects one request before the next acknowledgement", async () => {
  const { store, receive } = harness();
  const first = suggest(store, passage, "First");
  const second = suggest(store, passage, "Second");
  const refused = expect(first).rejects.toThrow("Nobody holds authoring");

  receive("review", { kind: "refused", reason: "Nobody holds authoring" });
  receive("review", {
    kind: "suggestion_status",
    id: "sg-2",
    state: "sent",
    holder: "Ada",
    away: false,
  });

  await refused;
  await expect(second).resolves.toMatchObject({ ok: true });
});

it("disconnect rejects pending review commands without recording a chat send", async () => {
  const { store } = harness();
  const pending = suggest(store, passage, "Review this");
  const rejected = expect(pending).rejects.toThrow(CONNECTION_LOST);
  store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null });
  await rejected;
  expect(store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("settled commands release listeners and the next command subscribes again", async () => {
  const { store, result, listeners } = harness();
  const first = sendMessage(store, "First");
  result(0);
  await first;
  expect([...listeners.values()].every((handlers) => handlers.size === 0)).toBe(true);

  const second = sendMessage(store, "Second");
  result(1);
  await second;
  expect([...listeners.values()].every((handlers) => handlers.size === 0)).toBe(true);
  expect(store.get(snapshotAtom).turns).toEqual([]);
});

it("getSessionBrief encodes the session and validates the response", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(briefFixture)));
  vi.stubGlobal("fetch", fetch);

  await expect(getSessionBrief("session /?&")).resolves.toEqual({ ok: true, value: briefFixture });
  expect(fetch).toHaveBeenCalledWith("/api/session/brief?session=session%20%2F%3F%26", {
    method: "GET",
    signal: undefined,
    credentials: "same-origin",
  });
});
