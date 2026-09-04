import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { connectStatusSocket, type SocketDeps } from "@/api/ws";
import { activityLogAtom, connectionAtom, snapshotAtom } from "@/state/atoms";

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

  fail(): void {
    this.onerror?.(new Event("error"));
  }

  open(): void {
    this.onopen?.(new Event("open"));
  }

  receive(data: string): void {
    this.onmessage?.({ data } as MessageEvent);
  }

  closeFromServer(reason = ""): void {
    this.onclose?.({ reason } as CloseEvent);
  }
}

type ScheduledTimer = { id: number; callback: () => void; delay: number; cancelled: boolean };

let nextTimerId = 1;
let timers: ScheduledTimer[] = [];

function fakeTimeout(callback: TimerHandler, delay?: number): number {
  const timer = {
    id: nextTimerId++,
    callback: callback as () => void,
    delay: delay ?? 0,
    cancelled: false,
  };
  timers.push(timer);
  return timer.id;
}

function fakeClearTimeout(id: number): void {
  const timer = timers.find((candidate) => candidate.id === id);
  if (timer) {
    timer.cancelled = true;
  }
}

function fireNextTimer(): void {
  const timer = timers.find((candidate) => !candidate.cancelled);
  if (!timer) {
    throw new Error("expected a scheduled reconnect");
  }
  timer.cancelled = true;
  timer.callback();
}

function dependencies(fetchImpl?: typeof fetch): SocketDeps {
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    fetch: fetchImpl ?? ((async () => ({ json: async () => fixture })) as unknown as typeof fetch),
    location: { protocol: "http:", host: "127.0.0.1:7373" },
    setTimeout: fakeTimeout as unknown as typeof setTimeout,
    clearTimeout: fakeClearTimeout as unknown as typeof clearTimeout,
  };
}

afterEach(() => {
  FakeSocket.instances = [];
  nextTimerId = 1;
  timers = [];
});

describe("status socket", () => {
  it("fetches the initial status and opens the expected socket URL", async () => {
    const store = createStore();
    const fetchImpl = vi.fn(async () => ({ json: async () => fixture })) as unknown as typeof fetch;
    const dispose = connectStatusSocket(store, dependencies(fetchImpl));

    expect(store.get(connectionAtom).phase).toBe("connecting");
    expect(FakeSocket.instances[0].url).toBe("ws://127.0.0.1:7373/ws");
    expect(fetchImpl).toHaveBeenCalledWith("/api/status", { cache: "no-store" });

    // Flush the fetch().then(json).then(receiveSnapshot) chain before the
    // socket's first frame arrives.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");

    FakeSocket.instances[0].open();
    expect(store.get(connectionAtom).phase).toBe("live");
    dispose();
  });

  it("logs and leaves the snapshot empty when the initial status fetch fails", async () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const failing = vi.fn(async () => {
      throw new Error("network down");
    }) as unknown as typeof fetch;
    const dispose = connectStatusSocket(store, dependencies(failing));

    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(error).toHaveBeenCalledWith("dashboard: status fetch failed", expect.any(Error));
    expect(store.get(snapshotAtom)).toBeNull();
    error.mockRestore();
    dispose();
  });

  it("applies valid websocket frames", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];

    socket.receive(JSON.stringify(fixture));

    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");
    expect(store.get(activityLogAtom)).toHaveLength(2);
    dispose();
  });

  it("keeps the last snapshot after a bad frame", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(JSON.stringify(fixture));
    const previous = store.get(snapshotAtom);
    const invalid = structuredClone(fixture) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";

    socket.receive(JSON.stringify(invalid));

    expect(store.get(connectionAtom).phase).toBe("error");
    expect(store.get(snapshotAtom)).toBe(previous);
    expect(error).toHaveBeenCalledWith("dashboard: bad frame", expect.any(Array));
    error.mockRestore();
    dispose();
  });

  it("returns to live once a valid frame follows a bad one, without restamping since on the next valid frame", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(JSON.stringify(fixture));
    const invalid = structuredClone(fixture) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";
    socket.receive(JSON.stringify(invalid));
    expect(store.get(connectionAtom).phase).toBe("error");

    socket.receive(JSON.stringify(fixture));

    expect(store.get(connectionAtom).phase).toBe("live");
    expect(store.get(connectionAtom).message).toBeUndefined();
    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");
    const since = store.get(connectionAtom).since;

    // A second consecutive valid frame must not call setConnection again -
    // `since` marks when the "live" phase began, not when a frame last
    // arrived, so it must stay pinned across repeat live frames.
    socket.receive(JSON.stringify(fixture));

    expect(store.get(connectionAtom).since).toBe(since);
    error.mockRestore();
    dispose();
  });

  it("backs off reconnects and cancels the pending reconnect when disposed", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    let socket = FakeSocket.instances[0];

    socket.open();
    socket.closeFromServer("lost connection");
    expect(store.get(connectionAtom)).toMatchObject({
      phase: "reconnecting",
      message: "lost connection",
    });
    expect(timers.map((timer) => timer.delay)).toEqual([1000]);

    for (const delay of [2000, 4000, 8000, 10000, 10000]) {
      fireNextTimer();
      socket = FakeSocket.instances.at(-1)!;
      socket.closeFromServer();
      expect(timers.at(-1)?.delay).toBe(delay);
    }

    const pending = timers.at(-1)!;
    dispose();

    expect(socket.closed).toBe(true);
    expect(pending.cancelled).toBe(true);
    pending.callback();
    expect(FakeSocket.instances).toHaveLength(6);
  });

  it("binds the global fetch/setTimeout/clearTimeout fallbacks to globalThis, not the client instance", () => {
    const originalFetch = globalThis.fetch;
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const receivers: { fetch?: unknown; setTimeout?: unknown; clearTimeout?: unknown } = {};

    globalThis.fetch = function (this: unknown) {
      receivers.fetch = this;
      return Promise.resolve({ json: async () => fixture });
    } as unknown as typeof fetch;
    globalThis.setTimeout = function (this: unknown) {
      receivers.setTimeout = this;
      return 1;
    } as unknown as typeof setTimeout;
    globalThis.clearTimeout = function (this: unknown) {
      receivers.clearTimeout = this;
    } as unknown as typeof clearTimeout;

    try {
      const store = createStore();
      const dispose = connectStatusSocket(store, {
        WebSocket: FakeSocket as unknown as typeof WebSocket,
        location: { protocol: "http:", host: "127.0.0.1:7373" },
      });
      const socket = FakeSocket.instances[0];

      // Triggers the initial fetch, a reconnect (setTimeout), then dispose
      // (clearTimeout), exercising all three fallbacks the same way.
      socket.closeFromServer("lost connection");
      dispose();

      expect(receivers.fetch).toBe(globalThis);
      expect(receivers.setTimeout).toBe(globalThis);
      expect(receivers.clearTimeout).toBe(globalThis);
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
    }
  });

  it("ignores a stale socket's close event once a newer socket has already opened", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const first = FakeSocket.instances[0];

    first.open();
    first.closeFromServer("lost connection");
    expect(timers).toHaveLength(1);

    // The reconnect timer fires and opens a fresh socket before the stale
    // socket's own close event arrives.
    fireNextTimer();
    expect(FakeSocket.instances).toHaveLength(2);

    first.closeFromServer("stale close, arrives late");

    expect(FakeSocket.instances).toHaveLength(2);
    expect(timers).toHaveLength(1);
    dispose();
  });

  it("reconnects after a socket error without double-scheduling its close", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];

    socket.fail();
    socket.closeFromServer("ignored after error");

    expect(store.get(connectionAtom)).toMatchObject({
      phase: "reconnecting",
      message: "socket error",
    });
    expect(timers).toHaveLength(1);
    dispose();
  });
});
