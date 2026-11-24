import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { chatMessageSchema } from "@/plan/api/schema-chat";
import { connectionAtom, unauthorizedAtom } from "@/plan/state/atoms";

class FakeSocket {
  static instances: FakeSocket[] = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  readyState = 0;
  readonly url: string;
  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  send() {}
  receive(data: unknown) {
    this.onmessage?.({ data: typeof data === "string" ? data : JSON.stringify(data) });
  }
}

const deps = { WebSocketImpl: FakeSocket as unknown as typeof WebSocket };
const fetchMock = vi.fn();

beforeEach(() => {
  vi.useFakeTimers();
  FakeSocket.instances = [];
  fetchMock.mockReset();
  fetchMock.mockImplementation(async () => new Response(JSON.stringify(server)));
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function openSocket(store: ReturnType<typeof createStore>) {
  const handle = connectPlanSocket(store, deps);
  await vi.advanceTimersByTimeAsync(0);
  const socket = FakeSocket.instances[0]!;
  socket.open();
  socket.receive(hello);
  return { handle, socket };
}

it.each([
  ["a frame that is not JSON", "not json"],
  ["a frame the schema rejects", { type: "presence", entries: [], extra: true }],
])("resyncs on %s: closes the socket and reconnects", async (_name, frame) => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const store = createStore();
  const { handle, socket } = await openSocket(store);
  expect(store.get(connectionAtom).phase).toBe("open");

  socket.receive(frame);

  expect(socket.readyState).toBe(3);
  expect(store.get(connectionAtom).phase).toBe("reconnecting");
  expect(warn.mock.calls.some((call) => call[0] === "plan socket: invalid frame")).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(FakeSocket.instances).toHaveLength(2);
  handle.dispose();
});

it("stops reconnecting once any REST call answered 401", async () => {
  const store = createStore();
  const { handle, socket } = await openSocket(store);
  const fetches = fetchMock.mock.calls.length;

  store.set(unauthorizedAtom, true);

  expect(socket.readyState).toBe(3);
  expect(store.get(connectionAtom).phase).toBe("unauthorized");
  await vi.advanceTimersByTimeAsync(30000);
  expect(FakeSocket.instances).toHaveLength(1);
  expect(fetchMock.mock.calls).toHaveLength(fetches);
  handle.dispose();
});

it("accepts a queue_returned frame with a cause on the chat wire form", () => {
  const wire = { type: "chat", frame: "queue_returned", texts: ["Held"], cause: "recovery" };

  expect(chatMessageSchema.safeParse(wire).success).toBe(true);
  expect(chatMessageSchema.safeParse({ ...wire, cause: "other" }).success).toBe(false);
});
