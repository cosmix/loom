import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { connectStatusSocket, type SocketDeps } from "@/api/ws";
import { connectionAtom, snapshotAtom } from "@/state/atoms";

const fixture = snapshotSchema.parse(fixtureJson);

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

function full(snapshot: unknown): string {
  return JSON.stringify({ t: "full", snapshot });
}

const tickMessage = JSON.stringify({
  t: "tick",
  at: "2026-09-04T12:00:01Z",
  tick_age: 1,
  stages: {},
});

let clock = 0;
let intervalCallback: (() => void) | undefined;
let intervalCleared = false;
let timeoutCallback: (() => void) | undefined;
let timeoutCleared = false;

function dependencies(): SocketDeps {
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    location: { protocol: "http:", host: "127.0.0.1:7373" },
    setTimeout: ((callback: () => void) => {
      timeoutCallback = callback;
      return 1;
    }) as unknown as typeof setTimeout,
    clearTimeout: (() => {
      timeoutCleared = true;
    }) as unknown as typeof clearTimeout,
    setInterval: ((callback: () => void) => {
      intervalCallback = callback;
      return 7;
    }) as unknown as typeof setInterval,
    clearInterval: ((id: number) => {
      if (id === 7) intervalCleared = true;
    }) as unknown as typeof clearInterval,
    now: () => clock,
  };
}

function tick(ms: number): void {
  clock += ms;
  intervalCallback?.();
}

afterEach(() => {
  FakeSocket.instances = [];
  clock = 0;
  intervalCallback = undefined;
  intervalCleared = false;
  timeoutCallback = undefined;
  timeoutCleared = false;
});

describe("status socket watchdog", () => {
  it("expires each silent handshake and ignores events from the retired socket", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const first = FakeSocket.instances[0];
    expect(store.get(connectionAtom).phase).toBe("connecting");

    tick(30000);
    expect(first.closed).toBe(true);
    expect(store.get(connectionAtom)).toMatchObject({
      phase: "reconnecting",
      message: "connection attempt timed out",
    });
    const since = store.get(connectionAtom).since;
    timeoutCallback?.();
    const second = FakeSocket.instances[1];
    first.open();
    first.receive(full({ ...fixture, generated_at: "2099-01-01T00:00:00Z" }));
    expect(store.get(connectionAtom).phase).toBe("reconnecting");
    expect(store.get(snapshotAtom)).toBeNull();
    tick(30000);
    expect(second.closed).toBe(true);
    expect(store.get(connectionAtom).since).toBe(since);
    dispose();
    expect(timeoutCleared).toBe(true);
    timeoutCallback?.();
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it("reconnects after 30s of silence on a socket that never closes", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();

    tick(30000);
    expect(store.get(connectionAtom).phase).toBe("live");

    tick(5000);
    expect(store.get(connectionAtom)).toMatchObject({
      phase: "reconnecting",
      message: "no data for 30s",
    });
    expect(socket.closed).toBe(true);
    dispose();
  });

  it("stays live while messages keep arriving, even unparseable ones", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();

    tick(25000);
    socket.receive(full(fixture));
    tick(25000);
    expect(store.get(connectionAtom).phase).toBe("live");

    socket.receive("not json");
    tick(25000);
    expect(store.get(connectionAtom).phase).toBe("error");
    expect(socket.closed).toBe(false);
    dispose();
  });

  it("ticks every second keep the socket live for a minute, and 31s of silence reconnects", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();
    socket.receive(full(fixture));

    for (let second = 0; second < 60; second++) {
      socket.receive(tickMessage);
      tick(1000);
    }
    expect(store.get(connectionAtom).phase).toBe("live");
    expect(socket.closed).toBe(false);
    expect(store.get(snapshotAtom)?.generated_at).toBe("2026-09-04T12:00:01Z");

    tick(30000);
    expect(store.get(connectionAtom)).toMatchObject({
      phase: "reconnecting",
      message: "no data for 30s",
    });
    expect(socket.closed).toBe(true);
    dispose();
  });

  it("checks on visibilitychange and stops watching once disposed", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();

    clock += 60000;
    document.dispatchEvent(new Event("visibilitychange"));
    expect(store.get(connectionAtom).phase).toBe("reconnecting");

    dispose();
    expect(intervalCleared).toBe(true);
    const phase = store.get(connectionAtom);
    clock += 60000;
    document.dispatchEvent(new Event("visibilitychange"));
    expect(store.get(connectionAtom)).toBe(phase);
  });
});
