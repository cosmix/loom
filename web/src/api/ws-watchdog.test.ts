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

let clock = 0;
let intervalCallback: (() => void) | undefined;
let intervalCleared = false;

function dependencies(): SocketDeps {
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    fetch: (async () => ({ json: async () => fixture })) as unknown as typeof fetch,
    location: { protocol: "http:", host: "127.0.0.1:7373" },
    setTimeout: (() => 1) as unknown as typeof setTimeout,
    clearTimeout: (() => undefined) as unknown as typeof clearTimeout,
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
});

describe("status socket watchdog", () => {
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
    socket.receive(JSON.stringify(fixture));
    tick(25000);
    expect(store.get(connectionAtom).phase).toBe("live");

    socket.receive("not json");
    tick(25000);
    expect(store.get(connectionAtom).phase).toBe("error");
    expect(socket.closed).toBe(false);
    dispose();
  });

  it("keeps the same snapshot object when the keepalive resends an identical frame", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();
    socket.receive(JSON.stringify(fixture));
    const stored = store.get(snapshotAtom);

    socket.receive(JSON.stringify(fixture));

    expect(store.get(snapshotAtom)).toBe(stored);
    expect(store.get(connectionAtom).phase).toBe("live");
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
