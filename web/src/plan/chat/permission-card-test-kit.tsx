import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai/vanilla";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Item } from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import { authoringAtom, connectionAtom, planSocketAtom, youAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { PermissionCard } from "./permission-card";

export type Permission = Extract<Item, { type: "permission" }>;
export const snapshot = chatSnapshotSchema.parse(fixture);
export const now = new Date("2026-10-28T10:00:00Z");

export function permission(fields: Partial<Permission> = {}): Permission {
  return {
    type: "permission",
    id: "permission-1",
    request_id: "request-1",
    tool: "Bash",
    input: '{"command":"ls doc/"}',
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2026-10-28T11:00:00Z",
    ...fields,
  };
}

export function harness(items = [permission()], viewer = false) {
  const store = createStore();
  store.set(youAtom, { public_id: viewer ? "reader" : "holder", name: "You" });
  store.set(authoringAtom, {
    holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(snapshotAtom, { ...snapshot, turns: [{ ...snapshot.turns[0], items }] });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: now.getTime() });
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const send = vi.fn((_message: unknown) => true);
  const socket = {
    send,
    subscribe(type: string, handler: (message: unknown) => void) {
      const handlers = listeners.get(type) ?? new Set();
      handlers.add(handler);
      listeners.set(type, handlers);
      return () => {
        handlers.delete(handler);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket as ReturnType<typeof connectPlanSocket>);
  function reply(error: string | null = null) {
    const message = send.mock.calls.at(-1)?.[0] as { client_id: string };
    act(() =>
      listeners.get("chat")?.forEach((handler) =>
        handler({
          type: "chat",
          frame: "result",
          client_id: message.client_id,
          ok: error === null,
          error,
        }),
      ),
    );
  }
  function show(item = items[0], inline = false) {
    return render(
      <Provider store={store}>
        <PermissionCard item={item} inline={inline} />
      </Provider>,
    );
  }
  return { store, send, reply, show };
}

export const card = () => screen.getByRole("region", { name: "Permission: Bash" });
export const payload = { type: "chat", command: "permission", request_id: "request-1" };

/** Pins the clock and unmounts after each test; call once at the top of a test file. */
export function useFakeClock() {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
}
