import { act } from "@testing-library/react";
import type { createStore } from "jotai";
import { vi, expect } from "vitest";

import backgroundFixture from "@/plan/api/fixtures/background.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import brief from "@/plan/api/fixtures/session-brief.json";
import { initClient } from "@/plan/api/client";
import { connectPlanSocket } from "@/plan/api/socket";

type Store = ReturnType<typeof createStore>;
type Obj = Record<string, unknown>;

/** The fixture's tasks: `a-run` running, `bshell01` running, `a-quiet` stalled, `a-done`, `bshell02` failed. */
export const TASKS = backgroundFixture.tasks;

// The plan socket's test seam: connectPlanSocket with a WebSocket stand-in.
class FakeSocket {
  static last: FakeSocket | undefined;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  constructor() {
    FakeSocket.last = this;
  }
  send = (): void => {};
  close = (): number => (this.readyState = 3);
}

/**
 * Connects `store` to a fake plan socket and returns `feed(tasks)`, which delivers a `background`
 * frame of the fixture (for `session`, default the fixture's) through the socket's message handler,
 * the way the server does. Call the returned `dispose` after the test.
 */
export async function connectBackground(store: Store, session = backgroundFixture.session) {
  initClient(store);
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  const socket = FakeSocket.last!;
  socket.readyState = 1;
  act(() => socket.onopen?.(new Event("open")));
  const feed = (tasks: Obj[]): void =>
    act(() => {
      const frame = { ...structuredClone(backgroundFixture), session, tasks };
      socket.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent);
    });
  return { feed, dispose: handle.dispose };
}

const json = (body: unknown): Response =>
  new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });

/** Answers the socket's `/api/server` probe and every other request with the session brief. */
export function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) =>
      new URL(String(input), "http://localhost").pathname === "/api/server"
        ? json({ ...helloFixture.server, read_only: [] })
        : json(brief),
    ),
  );
}
