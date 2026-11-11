import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import background from "@/plan/api/fixtures/background.json";
import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { backgroundAtom } from "@/plan/state/atoms";

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
    this.onmessage?.({ data: JSON.stringify(data) });
  }
}

const deps = { WebSocketImpl: FakeSocket as unknown as typeof WebSocket };

beforeEach(() => {
  vi.useFakeTimers();
  FakeSocket.instances = [];
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(async () => new Response(JSON.stringify(server))),
  );
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("stores a background frame without its tag and clears it on the next hello", async () => {
  const store = createStore();
  const handle = connectPlanSocket(store, deps);
  await vi.advanceTimersByTimeAsync(0);
  const socket = FakeSocket.instances[0]!;
  socket.open();
  socket.receive(hello);
  expect(store.get(backgroundAtom)).toBeNull();

  socket.receive(background);
  const { type: _type, ...frame } = background;
  expect(store.get(backgroundAtom)).toEqual(frame);
  expect(store.get(backgroundAtom)?.tasks).toHaveLength(5);

  socket.receive(hello);
  expect(store.get(backgroundAtom)).toBeNull();
  handle.dispose();
});

it("drops a background frame with an unknown task field", async () => {
  const store = createStore();
  const handle = connectPlanSocket(store, deps);
  vi.spyOn(console, "warn").mockImplementation(() => {});
  await vi.advanceTimersByTimeAsync(0);
  const socket = FakeSocket.instances[0]!;
  socket.open();
  socket.receive({ ...background, tasks: [{ ...background.tasks[0], extra: 1 }] });
  expect(store.get(backgroundAtom)).toBeNull();
  handle.dispose();
});
