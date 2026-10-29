import { act, cleanup, render } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Item, type SessionView } from "@/plan/api/schema-chat";
import { ChatAnnouncer } from "./announcer";
import { snapshotAtom } from "./model";

// The assertive region empties and rewrites after a short pause; the tests control that clock.
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const snapshot = chatSnapshotSchema.parse(fixture);
type Permission = Extract<Item, { type: "permission" }>;

function permission(
  id: string,
  tool: string,
  status: Permission["status"] = "pending",
): Permission {
  return {
    type: "permission",
    id,
    request_id: id,
    tool,
    input: "{}",
    status,
    deny_reason: null,
    note: null,
    expires_at: "2099-01-01T00:00:00Z",
  };
}

function setup() {
  const store = createStore();
  const view = render(
    <Provider store={store}>
      <ChatAnnouncer inSheet={false} />
    </Provider>,
  );
  const region = (kind: "alert" | "status") =>
    view.container.querySelector(`[data-chat-announcer="${kind}"]`)!;
  const arrive = (...items: Permission[]) =>
    act(() =>
      store.set(snapshotAtom, {
        ...snapshot,
        session: { ...snapshot.session!, state: "permission" },
        turns: [{ ...snapshot.turns[0], items }],
      }),
    );
  const settle = () => act(() => vi.advanceTimersByTime(1000));
  const permissions = (...items: Permission[]) => {
    arrive(...items);
    settle();
  };
  const conclude = (state: SessionView["state"], failure: SessionView["failure"] = null) =>
    act(() =>
      store.set(snapshotAtom, {
        ...snapshot,
        session: { ...snapshot.session!, state, failure },
        turns: [],
      }),
    );
  return { store, region, arrive, settle, permissions, conclude };
}

it("renders no live regions while there is no chat", () => {
  const h = setup();
  expect(h.region("alert")).toBeNull();
  expect(h.region("status")).toBeNull();
});

it("announces a permission request assertively, naming the tool, with no panel mounted", () => {
  const h = setup();
  h.conclude("starting");
  expect(h.region("alert").getAttribute("aria-live")).toBe("assertive");
  expect(h.region("alert").textContent).toBe("");
  h.permissions(permission("r1", "Bash"));
  expect(h.region("alert").textContent).toBe("Needs a decision: Bash");
});

it("announces each request once, however often the snapshot replays it", () => {
  const h = setup();
  h.permissions(permission("r1", "Bash"));
  h.permissions(permission("r1", "Bash", "allowed"));
  expect(h.region("alert").textContent).toBe("");
  // A replayed snapshot that still lists r1 as pending must not announce it again.
  h.permissions(permission("r1", "Bash"));
  expect(h.region("alert").textContent).toBe("");
  h.permissions(permission("r1", "Bash", "allowed"), permission("r2", "Edit"));
  expect(h.region("alert").textContent).toBe("Needs a decision: Edit");
});

it("announces a second request for the same tool by rewriting the region", () => {
  const h = setup();
  h.permissions(permission("r1", "Edit"));
  expect(h.region("alert").textContent).toBe("Needs a decision: Edit");
  h.arrive(permission("r1", "Edit"), permission("r2", "Edit"));
  // Identical words would leave the region unchanged and unspoken, so it empties first.
  expect(h.region("alert").textContent).toBe("");
  h.settle();
  expect(h.region("alert").textContent).toBe("Needs a decision: Edit");
});

it("announces a request that replaces a resolved one for the same tool", () => {
  const h = setup();
  h.permissions(permission("r1", "Edit"));
  h.arrive(permission("r1", "Edit", "allowed"), permission("r2", "Edit"));
  expect(h.region("alert").textContent).toBe("");
  h.settle();
  expect(h.region("alert").textContent).toBe("Needs a decision: Edit");
});

it("names every request that arrives inside one pause", () => {
  const h = setup();
  h.arrive(permission("r1", "Bash"));
  h.arrive(permission("r1", "Bash"), permission("r2", "Edit"));
  h.settle();
  expect(h.region("alert").textContent).toBe("Needs a decision: Bash, Edit");
});

it("says nothing for a request that was resolved before the pause ended", () => {
  const h = setup();
  h.arrive(permission("r1", "Bash"));
  h.arrive(permission("r1", "Bash", "denied"));
  h.settle();
  expect(h.region("alert").textContent).toBe("");
});

it("names every request that arrives together", () => {
  const h = setup();
  h.permissions(permission("r1", "Bash"), permission("r2", "Edit"));
  expect(h.region("alert").textContent).toBe("Needs a decision: Bash, Edit");
});

it("shows tool names with their format controls visible", () => {
  const h = setup();
  h.permissions(permission("r1", "Ba‮sh"));
  expect(h.region("alert").textContent).toBe("Needs a decision: Ba⟨U+202E⟩sh");
});

it.each([
  ["ended", null, "Chat ended"],
  ["failed", { kind: "spawn" as const, message: "no tmux" }, "The session could not start"],
  ["failed", { kind: "died" as const, message: "gone" }, "The session stopped unexpectedly"],
  ["working", null, ""],
])("announces a %s chat politely (%j)", (state, failure, words) => {
  const h = setup();
  h.conclude(state as SessionView["state"], failure);
  expect(h.region("status").getAttribute("aria-live")).toBe("polite");
  expect(h.region("status").textContent).toBe(words);
});

it("announces a chat that a late browser receives as previous", () => {
  const h = setup();
  act(() =>
    h.store.set(snapshotAtom, {
      session: null,
      turns: [],
      queue: [],
      previous: { session: { ...snapshot.session!, state: "ended" }, turns: [] },
    }),
  );
  expect(h.region("status").textContent).toBe("Chat ended");
});
