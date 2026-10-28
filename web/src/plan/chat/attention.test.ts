import { act, cleanup, renderHook } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Item } from "@/plan/api/schema-chat";
import { authoringAtom, youAtom } from "@/plan/state/atoms";
import { NOTIFICATIONS_STORAGE_KEY, notificationsEnabledAtom } from "@/state/notifications";
import { useAttention } from "./attention";
import { snapshotAtom } from "./model";

const snapshot = chatSnapshotSchema.parse(fixture);

class FakeNotification {
  static permission: NotificationPermission = "granted";
  static instances: FakeNotification[] = [];
  onclick: (() => void) | null = null;
  close = vi.fn();
  title: string;
  options: NotificationOptions;

  constructor(title: string, options: NotificationOptions) {
    this.title = title;
    this.options = options;
    FakeNotification.instances.push(this);
  }
}

function permission(id = "request-1"): Extract<Item, { type: "permission" }> {
  return {
    type: "permission",
    id,
    request_id: id,
    tool: "Bash",
    input: "ls doc/",
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2026-10-29T10:00:00Z",
  };
}

function harness({ optedIn = false, holder = true, open = true, items = [] as Item[] } = {}) {
  const store = createStore();
  store.set(notificationsEnabledAtom, optedIn);
  store.set(youAtom, { public_id: holder ? "holder" : "viewer", name: "You" });
  store.set(authoringAtom, {
    holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(snapshotAtom, { ...snapshot, turns: [{ ...snapshot.turns[0], open, items }] });
  const mounted = renderHook(useAttention, {
    wrapper: ({ children }: { children: ReactNode }) =>
      createElement(Provider, { store }, children),
  });
  const setItems = (next: Item[]) =>
    act(() =>
      store.set(snapshotAtom, (current) => ({
        ...current,
        turns: current.turns.map((turn) => ({ ...turn, items: next })),
      })),
    );
  const endTurn = () =>
    act(() =>
      store.set(snapshotAtom, (current) => ({
        ...current,
        turns: current.turns.map((turn) => ({ ...turn, open: false })),
      })),
    );
  return { store, setItems, endTurn, ...mounted };
}

let hidden = false;

function visibility(value: boolean): void {
  act(() => {
    hidden = value;
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

beforeEach(() => {
  document.title = "Project · loom";
  hidden = false;
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  FakeNotification.instances = [];
  FakeNotification.permission = "granted";
  vi.stubGlobal("Notification", FakeNotification);
  vi.stubGlobal("isSecureContext", true);
  vi.stubGlobal("focus", vi.fn());
  localStorage.removeItem(NOTIFICATIONS_STORAGE_KEY);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.removeItem(NOTIFICATIONS_STORAGE_KEY);
  document.title = "";
});

it("tab title prefixes", () => {
  const chat = harness();
  visibility(true);

  chat.setItems([permission()]);
  expect(document.title).toBe("(!) Project · loom");
  chat.setItems([]);
  chat.endTurn();
  expect(document.title).toBe("(✓) Project · loom");
  visibility(false);
  expect(document.title).toBe("Project · loom");
});

it("permission takes priority over a hidden turn completion", () => {
  const chat = harness();
  visibility(true);
  chat.endTurn();

  chat.setItems([permission()]);

  expect(document.title).toBe("(!) Project · loom");
});

it("showing the tab clears a waiting prefix until a new permission arrives", () => {
  const chat = harness({ items: [permission()] });
  expect(document.title).toBe("(!) Project · loom");

  visibility(false);
  chat.setItems([permission()]);
  expect(document.title).toBe("Project · loom");
  chat.setItems([permission(), permission("request-2")]);
  expect(document.title).toBe("(!) Project · loom");
});

it("a turn ending in the visible tab leaves the title alone", () => {
  const chat = harness();

  chat.endTurn();

  expect(document.title).toBe("Project · loom");
});

it("historical closed turns do not announce completion on mount", () => {
  hidden = true;

  harness({ optedIn: true, open: false });

  expect(document.title).toBe("Project · loom");
  expect(FakeNotification.instances).toHaveLength(0);
});

it("another session does not announce completion of the previous turn", () => {
  hidden = true;
  const chat = harness({ optedIn: true });

  act(() =>
    chat.store.set(snapshotAtom, (current) => ({
      ...current,
      session: { ...current.session!, id: "another-session" },
      turns: current.turns.map((turn) => ({ ...turn, open: false })),
    })),
  );

  expect(document.title).toBe("Project · loom");
  expect(FakeNotification.instances).toHaveLength(0);
});

it("notifications only when opted in", () => {
  const chat = harness();
  chat.setItems([permission()]);
  expect(FakeNotification.instances).toHaveLength(0);

  act(() => chat.store.set(notificationsEnabledAtom, true));
  chat.setItems([permission(), permission("request-2")]);

  expect(FakeNotification.instances).toHaveLength(1);
  expect(FakeNotification.instances[0].title).toBe("Chat needs a decision");
  expect(FakeNotification.instances[0].options.body).toBe("Needs a decision: Bash");
});

it("viewers receive no desktop notifications", () => {
  const chat = harness({ optedIn: true, holder: false });
  visibility(true);

  chat.setItems([permission()]);
  chat.endTurn();

  expect(FakeNotification.instances).toHaveLength(0);
  expect(document.title).toBe("(!) Project · loom");
});

it("a hidden turn completion notifies the holder once and focuses on click", () => {
  const chat = harness({ optedIn: true });
  visibility(true);

  chat.endTurn();
  chat.setItems([]);

  expect(FakeNotification.instances).toHaveLength(1);
  const notification = FakeNotification.instances[0];
  expect(notification.title).toBe("Chat turn ended");
  notification.onclick?.();
  expect(window.focus).toHaveBeenCalledOnce();
  expect(notification.close).toHaveBeenCalledOnce();
});

it("denied browser permission suppresses notifications", () => {
  FakeNotification.permission = "denied";
  const chat = harness({ optedIn: true });

  chat.setItems([permission()]);

  expect(FakeNotification.instances).toHaveLength(0);
  expect(document.title).toBe("(!) Project · loom");
});

it("unmount restores the title and removes attention subscriptions", () => {
  const chat = harness({ optedIn: true, items: [permission()] });

  chat.unmount();
  visibility(true);
  chat.setItems([permission("new-request")]);
  chat.endTurn();

  expect(document.title).toBe("Project · loom");
  expect(FakeNotification.instances).toHaveLength(0);
});
