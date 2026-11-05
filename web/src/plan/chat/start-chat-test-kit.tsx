import { act, cleanup, render } from "@testing-library/react";
import { Provider } from "jotai";
import { createStore } from "jotai/vanilla";
import type { ReactNode } from "react";
import { afterEach, beforeEach, vi } from "vitest";

import brief from "@/plan/api/fixtures/session-brief.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { planMessageSchema, type OfferView } from "@/plan/api/schema";
import {
  chatSnapshotSchema,
  sessionViewSchema,
  type SessionView,
  type Turn,
} from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import {
  authoringAtom,
  checklistAtom,
  connectionAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { snapshotAtom } from "./model";

export const session = sessionViewSchema.parse({ ...snapshotFixture.session, state: "starting" });
export const offer: OfferView = {
  id: "start-vision",
  kind: "start_vision",
  label: "Start the product vision",
  target: null,
};
/** A chat the user wrote in: only such a chat has a closing write, so only it asks before ending. */
export const wroteTurns: Turn[] = [
  {
    ...chatSnapshotSchema.parse(snapshotFixture).turns[0]!,
    items: [
      { type: "user", id: "wrote", text: "Hi", attachments: [], delivery: "sent", origin: null },
    ],
  },
];
export const disposers: Array<() => void> = [];

export function harness(current: SessionView | null = null, turns: Turn[] = wroteTurns) {
  const store = createStore();
  const hello = planMessageSchema.parse(helloFixture);
  if (hello.type !== "hello") throw new Error("Expected the hello fixture");
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, {
    holder: {
      public_id: hello.you.public_id,
      name: hello.you.name,
      away_since: null,
      frees_at: null,
    },
  });
  store.set(checklistAtom, { ...hello.checklists[0], primary: offer });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  store.set(snapshotAtom, { session: current, turns, queue: [], previous: null });
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const send = vi.fn((_message: unknown) => true);
  const socket = {
    send,
    subscribe(type: string, handler: (message: unknown) => void) {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
  };
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  const result = (index = 0, error: string | null = null) => {
    const { client_id } = send.mock.calls[index][0] as { client_id: string };
    act(() =>
      listeners
        .get("chat")
        ?.forEach((handler) =>
          handler({ type: "chat", frame: "result", client_id, ok: error === null, error }),
        ),
    );
  };
  const mount = (node: ReactNode) => render(<Provider store={store}>{node}</Provider>);
  return { store, send, result, mount };
}

/** Registers the per-test reset; call once at the top of each start-chat test file. */
export function installStartChatLifecycle() {
  beforeEach(() => {
    localStorage.clear();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(async () => new Response(JSON.stringify(brief))),
    );
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });

  afterEach(() => {
    cleanup();
    disposers.splice(0).forEach((dispose) => dispose());
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });
}
