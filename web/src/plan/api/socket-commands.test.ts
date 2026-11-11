import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import background from "@/plan/api/fixtures/background.json";
import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import commands from "@/plan/api/fixtures/session-commands.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { installChatSubscription } from "@/plan/chat/model";
import { backgroundAtom, planSocketAtom } from "@/plan/state/atoms";
import { commandsAtom } from "@/plan/state/commands-atom";

const SESSION = snapshot.session.id;
const OTHER = "00000000-0000-4000-8000-0000000000ff";

class FakeSocket {
  static instances: FakeSocket[] = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  readyState = 0;
  constructor() {
    FakeSocket.instances.push(this);
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  close() {
    this.readyState = 3;
  }
  send() {}
  receive(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }
}

let cleanups: Array<() => void> = [];

/** An open plan socket after its hello and the idle chat snapshot of the fixture's session. */
async function connect() {
  const store = createStore();
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  store.set(planSocketAtom, handle);
  cleanups.push(handle.dispose, installChatSubscription(store));
  await vi.advanceTimersByTimeAsync(0);
  const socket = FakeSocket.instances[0]!;
  socket.open();
  socket.receive(hello);
  const session = { ...snapshot.session, state: "idle", queued: 0 };
  const idle = { ...snapshot, session, turns: [], queue: [], previous: null };
  socket.receive({ ...idle, type: "chat", frame: "snapshot" });
  return { store, socket };
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeSocket.instances = [];
  // Every route answers the server's info, so the commands route gives no list of its own.
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(async () => new Response(JSON.stringify(server))),
  );
});

afterEach(() => {
  cleanups.splice(0).forEach((stop) => stop());
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("hands a commands frame to the commands atom and clears it on the next hello", async () => {
  const { store, socket } = await connect();
  expect(store.get(commandsAtom)).toMatchObject({
    session: SESSION,
    state: "loading",
    entries: [],
  });

  socket.receive({ type: "commands", session: SESSION, ...commands });
  expect(store.get(commandsAtom)).toEqual({ session: SESSION, ...commands });

  socket.receive(hello);
  expect(store.get(commandsAtom).entries).toEqual([]);
});

it("drops a commands frame with an unknown entry field", async () => {
  const { store, socket } = await connect();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  const entries = [{ ...commands.entries[0], extra: 1 }];
  socket.receive({ type: "commands", session: SESSION, ...commands, entries });
  expect(store.get(commandsAtom).entries).toEqual([]);
});

it("ignores the frames of a session other than the chat's", async () => {
  const { store, socket } = await connect();
  socket.receive({ type: "commands", session: SESSION, ...commands });
  socket.receive(background);
  expect(background.session).toBe(SESSION);

  socket.receive({ type: "commands", session: OTHER, ...commands, generation: 7, entries: [] });
  socket.receive({ ...background, session: OTHER, tasks: [] });
  expect(store.get(commandsAtom)).toEqual({ session: SESSION, ...commands });
  expect(store.get(backgroundAtom)?.session).toBe(SESSION);
  expect(store.get(backgroundAtom)?.tasks).toHaveLength(background.tasks.length);
});
