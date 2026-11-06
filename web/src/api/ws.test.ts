import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

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

function full(snapshot: unknown): string {
  return JSON.stringify({ t: "full", snapshot });
}

function dependencies(): SocketDeps {
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
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
  it("opens the expected socket URL and goes live on open", () => {
    const store = createStore();
    const fetchImpl = vi.fn();
    vi.stubGlobal("fetch", fetchImpl);
    const dispose = connectStatusSocket(store, dependencies());

    expect(store.get(connectionAtom).phase).toBe("connecting");
    expect(FakeSocket.instances[0].url).toBe("ws://127.0.0.1:7373/ws");
    expect(store.get(snapshotAtom)).toBeNull();

    FakeSocket.instances[0].open();
    expect(store.get(connectionAtom).phase).toBe("live");
    // The socket's first message is the whole snapshot: nothing is fetched.
    expect(fetchImpl).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
    dispose();
  });

  it("applies valid websocket frames", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];

    socket.receive(full(fixture));
    const next = structuredClone(fixture);
    next.generated_at = "2026-09-04T12:00:01Z";
    next.status.stages.find((stage) => stage.id === "server")!.status = "completed";
    socket.receive(full(next));

    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");
    expect(store.get(snapshotAtom)?.generated_at).toBe("2026-09-04T12:00:01Z");
    dispose();
  });

  it("applies deltas after the first full snapshot", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();
    socket.receive(full(fixture));

    socket.receive(
      JSON.stringify({
        t: "tick",
        at: "2026-09-04T12:00:02Z",
        tick_age: 9,
        stages: { server: { elapsed: 1000 } },
      }),
    );
    socket.receive(JSON.stringify({ t: "state", at: "2026-09-04T12:00:03Z", notice: "degraded" }));

    const snapshot = store.get(snapshotAtom);
    expect(snapshot?.status.stages.find((stage) => stage.id === "server")?.elapsed_secs).toBe(1000);
    expect(snapshot?.tick_age_secs).toBe(9);
    expect(snapshot?.notice).toBe("degraded");
    expect(snapshot?.generated_at).toBe("2026-09-04T12:00:03Z");
    expect(store.get(connectionAtom).phase).toBe("live");
    dispose();
  });

  it("drops a delta that arrives before the socket's first full snapshot", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.open();

    socket.receive(JSON.stringify({ t: "state", at: "2026-09-04T12:00:03Z", notice: "early" }));

    expect(store.get(snapshotAtom)).toBeNull();
    expect(error).toHaveBeenCalledWith("dashboard: frame before the first full snapshot", "state");
    error.mockRestore();
    dispose();
  });

  it("applies a frame whose only defect is an invalid tokens block, with tokens absent", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    const invalid = structuredClone(fixture);
    invalid.status.tokens!.plan.rate.cache_write = -1;

    socket.receive(full(invalid));

    expect(store.get(connectionAtom).phase).toBe("live");
    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");
    expect(store.get(snapshotAtom)?.status.stages).toHaveLength(fixture.status.stages.length);
    expect(store.get(snapshotAtom)?.status.tokens).toBeUndefined();
    dispose();
  });

  it("applies a tokens frame whose block is invalid as no tokens", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(full(fixture));
    const invalid = structuredClone(fixture.status.tokens!);
    invalid.plan.rate.cache_write = -1;

    socket.receive(JSON.stringify({ t: "tokens", tokens: invalid }));

    expect(store.get(connectionAtom).phase).toBe("live");
    expect(store.get(snapshotAtom)?.status.tokens).toBeUndefined();
    expect(store.get(snapshotAtom)?.status.stages).toHaveLength(fixture.status.stages.length);
    error.mockRestore();
    dispose();
  });

  it("keeps the last snapshot after a bad frame", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(full(fixture));
    const previous = store.get(snapshotAtom);
    const invalid = structuredClone(fixture) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";

    socket.receive(full(invalid));

    expect(store.get(connectionAtom).phase).toBe("error");
    expect(store.get(snapshotAtom)).toBe(previous);
    expect(error).toHaveBeenCalledWith("dashboard: bad frame", expect.any(Array));
    error.mockRestore();
    dispose();
  });

  it("drops deltas after a rejected frame and stays in error until a full parses", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(full(fixture));
    const previous = store.get(snapshotAtom);
    socket.receive(JSON.stringify({ t: "state", at: "2026-01-01T00:00:00Z", stages: { x: {} } }));
    expect(store.get(connectionAtom).phase).toBe("error");

    socket.receive(
      JSON.stringify({ t: "tick", at: "2026-01-01T00:00:01Z", tick_age: 1, stages: {} }),
    );

    expect(store.get(connectionAtom).phase).toBe("error");
    expect(store.get(snapshotAtom)).toBe(previous);
    socket.receive(full(fixture));
    expect(store.get(connectionAtom).phase).toBe("live");
    error.mockRestore();
    dispose();
  });

  it("returns to live once a valid frame follows a bad one, without restamping since on the next valid frame", () => {
    const store = createStore();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dispose = connectStatusSocket(store, dependencies());
    const socket = FakeSocket.instances[0];
    socket.receive(full(fixture));
    const invalid = structuredClone(fixture) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";
    socket.receive(full(invalid));
    expect(store.get(connectionAtom).phase).toBe("error");

    socket.receive(full(fixture));

    expect(store.get(connectionAtom).phase).toBe("live");
    expect(store.get(connectionAtom).message).toBeUndefined();
    expect(store.get(snapshotAtom)?.status.plan_name).toBe("Web Dashboard Fixture");
    const since = store.get(connectionAtom).since;

    // A second consecutive valid frame must not call setConnection again -
    // `since` marks when the "live" phase began, not when a frame last
    // arrived, so it must stay pinned across repeat live frames.
    socket.receive(full(fixture));

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

  it("keeps a fixed since across successive reconnect attempts within one outage", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    let socket = FakeSocket.instances[0];

    socket.open();
    socket.closeFromServer("lost connection");
    expect(store.get(connectionAtom).phase).toBe("reconnecting");
    const since = store.get(connectionAtom).since;

    for (let i = 0; i < 3; i++) {
      fireNextTimer();
      socket = FakeSocket.instances.at(-1)!;
      socket.closeFromServer();
      expect(store.get(connectionAtom).phase).toBe("reconnecting");
      expect(store.get(connectionAtom).since).toBe(since);
    }

    dispose();
  });

  it("flips to offline on the 5th consecutive reconnect failure, keeping since fixed", () => {
    const store = createStore();
    const dispose = connectStatusSocket(store, dependencies());
    let socket = FakeSocket.instances[0];

    socket.open();
    socket.closeFromServer("lost connection"); // failure 1: reconnecting
    const since = store.get(connectionAtom).since;

    for (let i = 0; i < 3; i++) {
      fireNextTimer();
      socket = FakeSocket.instances.at(-1)!;
      socket.closeFromServer(); // failures 2-4: still reconnecting
      expect(store.get(connectionAtom).phase).toBe("reconnecting");
    }

    fireNextTimer();
    socket = FakeSocket.instances.at(-1)!;
    socket.closeFromServer(); // failure 5: offline

    expect(store.get(connectionAtom).phase).toBe("offline");
    expect(store.get(connectionAtom).since).toBe(since);

    dispose();
  });

  it("binds the global setTimeout/clearTimeout fallbacks to globalThis, not the client instance", () => {
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const receivers: { setTimeout?: unknown; clearTimeout?: unknown } = {};

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

      // Triggers a reconnect (setTimeout), then dispose (clearTimeout),
      // exercising both fallbacks the same way.
      socket.closeFromServer("lost connection");
      dispose();

      expect(receivers.setTimeout).toBe(globalThis);
      expect(receivers.clearTimeout).toBe(globalThis);
    } finally {
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
