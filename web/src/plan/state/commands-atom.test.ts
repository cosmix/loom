import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import ready from "@/plan/api/fixtures/session-commands.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { installChatSubscription } from "@/plan/chat/model";
import { planSocketAtom } from "@/plan/state/atoms";
import { commandsAtom } from "@/plan/state/commands-atom";

type Obj = Record<string, unknown>;

const SESSION = snapshot.session.id;
const OTHER = "00000000-0000-4000-8000-0000000000ff";
const NATIVE = ready.entries.filter((entry) => entry.source === "built-in");
const LOADING = { state: "loading", generation: null, reason: null, entries: NATIVE };

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

/** What `/api/session/commands` answers; a test replaces it, null makes the route fail. */
let route: Obj | null = LOADING;
let cleanups: Array<() => void> = [];

/** The query of every request the commands route got, in order. */
const asked = (): string[] =>
  vi
    .mocked(fetch)
    .mock.calls.map(([input]) => new URL(String(input), "http://localhost"))
    .filter((url) => url.pathname === "/api/session/commands")
    .map((url) => url.search);

/** A chat snapshot frame of session `id`, idle and empty. */
function chat(id: string): Obj {
  const session = { ...snapshot.session, id, state: "idle", queued: 0 };
  return {
    ...snapshot,
    type: "chat",
    frame: "snapshot",
    session,
    turns: [],
    queue: [],
    previous: null,
  };
}

const frame = (session: string, fields: Obj = {}): Obj => ({
  type: "commands",
  session,
  ...ready,
  ...fields,
});

/** A store on an open plan socket after its hello, with a mounted reader as the composer is. */
async function connect() {
  const store = createStore();
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  store.set(planSocketAtom, handle);
  cleanups.push(
    handle.dispose,
    installChatSubscription(store),
    store.sub(commandsAtom, () => {}),
  );
  await vi.waitFor(() => expect(FakeSocket.instances).toHaveLength(1));
  const socket = FakeSocket.instances[0]!;
  socket.open();
  socket.receive(hello);
  return { store, socket, view: () => store.get(commandsAtom) };
}

/** Lets every answer under way reach the store. */
const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 20));

beforeEach(() => {
  FakeSocket.instances = [];
  route = LOADING;
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (input) => {
      const path = new URL(String(input), "http://localhost").pathname;
      if (path === "/api/server") return new Response(JSON.stringify(server));
      if (route === null) return new Response("{}", { status: 500 });
      return new Response(JSON.stringify(route));
    }),
  );
});

afterEach(() => {
  cleanups.splice(0).forEach((stop) => stop());
  vi.unstubAllGlobals();
});

it("holds the route's list for no session until a chat has one", async () => {
  const { view } = await connect();
  await vi.waitFor(() => expect(view().entries).toEqual(NATIVE));
  expect(view()).toMatchObject({ session: null, state: "loading", generation: null });
  expect(asked()).toEqual([""]);
});

it("a commands frame replaces the list and a stale generation does not", async () => {
  const { socket, view } = await connect();
  socket.receive(chat(SESSION));
  await settle();
  socket.receive(frame(SESSION, { generation: 2, entries: ready.entries.slice(0, 2) }));
  expect(view()).toMatchObject({ session: SESSION, state: "ready", generation: 2 });
  expect(view().entries).toHaveLength(2);

  socket.receive(frame(SESSION, { generation: 1 }));
  expect(view().generation).toBe(2);
  expect(view().entries).toHaveLength(2);
  // A list that is not ready is older than any ready one of the same connection.
  socket.receive(frame(SESSION, LOADING));
  expect(view().generation).toBe(2);

  socket.receive(frame(SESSION, { generation: 3 }));
  expect(view().generation).toBe(3);
  expect(view().entries).toEqual(ready.entries);
});

it("a frame for another session is ignored", async () => {
  const { socket, view } = await connect();
  socket.receive(chat(SESSION));
  socket.receive(frame(SESSION, { generation: 4 }));
  socket.receive(frame(OTHER, { generation: 9, entries: [] }));
  await settle();
  expect(view()).toMatchObject({ session: SESSION, generation: 4 });
  expect(view().entries).toEqual(ready.entries);
});

it("a session change and an epoch bump refetch", async () => {
  const { socket, view } = await connect();
  await vi.waitFor(() => expect(asked()).toEqual([""]));
  route = ready;
  socket.receive(chat(SESSION));
  await vi.waitFor(() => expect(asked()).toEqual(["", `?session=${SESSION}`]));
  await vi.waitFor(() => expect(view()).toMatchObject({ session: SESSION, state: "ready" }));

  // A delta of the same session asks for nothing.
  socket.receive(chat(SESSION));
  await settle();
  expect(asked()).toHaveLength(2);

  // A second hello is a resync: the epoch bumps and the list is asked for again.
  route = { ...ready, generation: 5 };
  socket.receive(hello);
  await vi.waitFor(() => expect(asked()).toHaveLength(3));
  await vi.waitFor(() => expect(view().generation).toBe(5));

  route = LOADING;
  socket.receive(chat(OTHER));
  await vi.waitFor(() => expect(asked().at(-1)).toBe(`?session=${OTHER}`));
  await vi.waitFor(() => expect(view()).toMatchObject({ session: OTHER, state: "loading" }));
});

it("the newer of the route's answer and the socket's frame is held", async () => {
  route = { ...ready, generation: 2 };
  const { socket, view } = await connect();
  socket.receive(chat(SESSION));
  socket.receive(frame(SESSION, { generation: 1, entries: [] }));
  expect(view().generation).toBe(1);
  await vi.waitFor(() => expect(view().generation).toBe(2));
  expect(view().entries).toEqual(ready.entries);

  // A resync drops the frame: a restarted server counts its generations from one again.
  route = { ...ready, generation: 1, entries: NATIVE };
  socket.receive(hello);
  await vi.waitFor(() => expect(view()).toMatchObject({ generation: 1, entries: NATIVE }));
});

it("a route that fails reads as unavailable and gives way to a frame", async () => {
  route = null;
  const { socket, view } = await connect();
  socket.receive(chat(SESSION));
  await vi.waitFor(() => expect(view()).toMatchObject({ session: SESSION, state: "unavailable" }));
  expect(view().entries).toEqual([]);
  socket.receive(frame(SESSION, LOADING));
  expect(view()).toMatchObject({ state: "loading", entries: NATIVE });
});

it("a store whose socket never said hello asks for nothing", async () => {
  const store = createStore();
  cleanups.push(store.sub(commandsAtom, () => {}));
  await settle();
  expect(fetch).not.toHaveBeenCalled();
  expect(store.get(commandsAtom)).toMatchObject({ session: null, state: "loading", entries: [] });
});
