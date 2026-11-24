import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { connectionAtom } from "@/plan/state/atoms";

class FakeSocket {
  static instances: FakeSocket[] = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  readyState = 0;
  sent: string[] = [];
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
  send(data: string) {
    this.sent.push(data);
  }
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
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const INVALID_FRAMES = ["not json", JSON.stringify({ type: "presence", entries: [], extra: true })];

describe("plan socket contract", () => {
  it("an invalid frame surfaces as a connection problem", async () => {
    for (const raw of INVALID_FRAMES) {
      FakeSocket.instances = [];
      const store = createStore();
      const handle = connectPlanSocket(store, deps);
      await vi.advanceTimersByTimeAsync(0);
      const socket = FakeSocket.instances[0];
      socket.open();
      socket.receive(hello);

      socket.onmessage?.({ data: raw });

      expect(socket.readyState, raw).toBe(3);
      expect(store.get(connectionAtom).phase, raw).toBe("reconnecting");
      await vi.advanceTimersByTimeAsync(1000);
      expect(FakeSocket.instances.length, raw).toBeGreaterThanOrEqual(2);
      handle.dispose();
    }
  });
});
