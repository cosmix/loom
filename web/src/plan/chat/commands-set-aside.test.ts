import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { connectPlanSocket } from "@/plan/api/socket";
import { connectionAtom, planSocketAtom } from "@/plan/state/atoms";
import { discardKickoff, setAsideKickoff } from "./commands";
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

const SESSION = "11111111-1111-4111-8111-111111111111";

it("set aside sends now: false by default and now: true for Stop now", async () => {
  const { store, send, result } = harness();

  const gentle = setAsideKickoff(store, SESSION);
  const immediate = setAsideKickoff(store, SESSION, true);

  expect(send.mock.calls[0]![0]).toMatchObject({ command: "set_aside_kickoff", now: false });
  expect(send.mock.calls[1]![0]).toMatchObject({ command: "set_aside_kickoff", now: true });
  result(0);
  result(1);
  await Promise.all([gentle, immediate]);
});

it("discard sends delete_files exactly as the caller chose", async () => {
  const { store, send, result } = harness();

  const keep = discardKickoff(store, SESSION, false);
  const remove = discardKickoff(store, SESSION, true);

  expect(send.mock.calls[0]![0]).toMatchObject({ command: "discard_kickoff", delete_files: false });
  expect(send.mock.calls[1]![0]).toMatchObject({ command: "discard_kickoff", delete_files: true });
  result(0);
  result(1);
  await Promise.all([keep, remove]);
});
