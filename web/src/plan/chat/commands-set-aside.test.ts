import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { connectPlanSocket } from "@/plan/api/socket";
import { connectionAtom, planSocketAtom } from "@/plan/state/atoms";
import { setAsideKickoff } from "./commands";
import { installChatSubscription, queueReturnedAtom } from "./model";

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

it("setting a kickoff aside permits queue_returned after its result, once", async () => {
  const { store, receive, result } = harness();
  const dispose = installChatSubscription(store);
  const pending = setAsideKickoff(store, "11111111-1111-4111-8111-111111111111");

  result(0);
  receive("chat", { frame: "queue_returned", texts: ["Typed while it ran"] });
  receive("chat", { frame: "queue_returned", texts: ["Unsolicited"] });

  await pending;
  expect(store.get(queueReturnedAtom)).toEqual(["Typed while it ran"]);
  dispose();
});
