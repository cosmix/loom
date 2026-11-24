import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { connectStatusSocket, type SocketDeps } from "@/api/ws";
import { connectionAtom, snapshotAtom } from "@/state/atoms";

const snapshot = snapshotSchema.parse(fixtureJson);

class FakeSocket {
  static instances: FakeSocket[] = [];

  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  closed = false;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }

  open(): void {
    this.onopen?.(new Event("open"));
  }

  receive(data: string): void {
    this.onmessage?.({ data } as MessageEvent);
  }
}

// Timers are recorded and never fired: the contract needs no reconnect or watchdog check.
function dependencies(): SocketDeps {
  let nextId = 1;
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    location: { protocol: "http:", host: "127.0.0.1:7373" },
    setTimeout: (() => nextId++) as unknown as typeof setTimeout,
    clearTimeout: (() => {}) as unknown as typeof clearTimeout,
    setInterval: (() => nextId++) as unknown as typeof setInterval,
    clearInterval: (() => {}) as unknown as typeof clearInterval,
    now: () => 0,
  };
}

const full = JSON.stringify({ t: "full", snapshot });
const tick = JSON.stringify({ t: "tick", at: "2026-09-04T12:00:01Z", tick_age: 1, stages: {} });

describe("status socket contract", () => {
  it("reports live only after the first applied snapshot", () => {
    FakeSocket.instances = [];
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[FakeSocket.instances.length - 1];

    socket.open();
    expect(store.get(connectionAtom).phase).toBe("connecting");

    socket.receive(tick);
    expect(store.get(connectionAtom).phase).toBe("connecting");
    expect(store.get(snapshotAtom)).toBeNull();

    socket.receive(full);
    expect(store.get(connectionAtom).phase).toBe("live");
    expect(store.get(snapshotAtom)).not.toBeNull();
    dispose();
  });
});
