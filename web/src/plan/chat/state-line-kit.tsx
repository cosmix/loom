import { act, cleanup, render } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type Item, type SessionView, type Turn } from "@/plan/api/schema-chat";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { RunHeader } from "./run-header";
import { StateLine } from "./state-line";

const snapshot = chatSnapshotSchema.parse(snapshotFixture);
export const base = snapshot.session!;
export const now = new Date("2026-10-28T10:00:00Z");
export const since = "2026-10-28T09:59:19Z";
export const server = { ...serverInfoSchema.parse(helloFixture.server), read_only: [] };
export const stops: Array<() => void> = [];

export function session(fields: Partial<SessionView> = {}): SessionView {
  return {
    ...base,
    state: "idle",
    state_since: since,
    working_since: since,
    subagents_running: 0,
    last_activity: since,
    ...fields,
  };
}

export function turn(items: Item[] = [], fields: Partial<Turn> = {}): Turn {
  return {
    ...snapshot.turns[0],
    id: "current",
    open: true,
    started_at: since,
    items,
    tokens: { input: 1000, cache_write: 200, cache_read: 999999, output: 300 },
    ...fields,
  };
}

export function permission(tool: string, id = tool): Extract<Item, { type: "permission" }> {
  return {
    type: "permission",
    id,
    request_id: id,
    tool,
    input: "{}",
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2026-10-28T11:00:00Z",
  };
}

export function tool(summary: string): Extract<Item, { type: "tool" }> {
  const sample = snapshot.turns[0].items.find((item) => item.type === "tool");
  if (!sample || sample.type !== "tool") throw new Error("Missing tool fixture");
  return { ...sample, summary };
}

export function harness(current: SessionView | null = session(), turns = [turn()]) {
  const store = createStore();
  store.set(serverInfoAtom, server);
  store.set(youAtom, { public_id: "holder", name: "Dana" });
  store.set(authoringAtom, {
    holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(snapshotAtom, { session: current, turns, queue: [], previous: null });
  function setSession(next: SessionView | null) {
    act(() => store.set(snapshotAtom, { ...store.get(snapshotAtom), session: next }));
  }
  function setTurns(next: Turn[]) {
    act(() => store.set(snapshotAtom, { ...store.get(snapshotAtom), turns: next }));
  }
  function show() {
    return render(
      <Provider store={store}>
        <StateLine />
      </Provider>,
    );
  }
  function header(value = current!) {
    return render(
      <Provider store={store}>
        <RunHeader session={value} />
      </Provider>,
    );
  }
  return { store, setSession, setTurns, show, header };
}

export function setupStateLineTests() {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    // The Starting row loads the session brief; it never answers here.
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    stops.splice(0).forEach((stop) => stop());
    vi.useRealTimers();
  });
}
