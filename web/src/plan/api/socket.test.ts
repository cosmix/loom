import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import {
  checklistAtom,
  connectionAtom,
  connectionEpochAtom,
  presenceAtom,
  unauthorizedAtom,
} from "@/plan/state/atoms";

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
  fail() {
    this.onerror?.();
  }
}

/** A WebSocket constructor the browser rejects (for example a blocked mixed-content URL). */
class ThrowingSocket {
  static attempts = 0;
  constructor() {
    ThrowingSocket.attempts += 1;
    throw new DOMException("blocked", "SecurityError");
  }
}

const deps = { WebSocketImpl: FakeSocket as unknown as typeof WebSocket };
const response = () => new Response(JSON.stringify(server));
async function flush() {
  await vi.advanceTimersByTimeAsync(0);
}
function latest() {
  return FakeSocket.instances[FakeSocket.instances.length - 1];
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeSocket.instances = [];
  ThrowingSocket.attempts = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(async () => response()),
  );
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("plan socket", () => {
  it("probes before connecting and follows the capped retry delays", async () => {
    const store = createStore();
    const handle = connectPlanSocket(store, deps);
    expect(FakeSocket.instances).toHaveLength(0);
    await flush();
    expect(latest().url).toBe(
      `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`,
    );
    for (const [index, delay] of [1000, 2000, 5000, 10000, 10000].entries()) {
      latest().close();
      expect(store.get(connectionAtom).phase).toBe("reconnecting");
      await vi.advanceTimersByTimeAsync(delay - 1);
      expect(FakeSocket.instances).toHaveLength(index + 1);
      await vi.advanceTimersByTimeAsync(1);
      expect(FakeSocket.instances).toHaveLength(index + 2);
    }
    handle.dispose();
  });

  it("retries rejected probes and resets the backoff only after hello", async () => {
    vi.mocked(fetch)
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockRejectedValueOnce(new TypeError("Failed to fetch"));
    const handle = connectPlanSocket(createStore(), deps);
    await flush();
    await vi.advanceTimersByTimeAsync(1000);
    expect(FakeSocket.instances).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(2000);
    expect(FakeSocket.instances).toHaveLength(1);
    latest().close();
    await vi.advanceTimersByTimeAsync(4999);
    expect(FakeSocket.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    latest().open();
    latest().receive(hello);
    latest().close();
    await vi.advanceTimersByTimeAsync(1000);
    expect(FakeSocket.instances).toHaveLength(3);
    handle.dispose();
  });

  it("stops on a 401 and preserves whether the page was ever open", async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(response())
      .mockResolvedValueOnce(new Response("{}", { status: 401 }));
    const store = createStore();
    const handle = connectPlanSocket(store, deps);
    await flush();
    latest().open();
    latest().close();
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.get(unauthorizedAtom)).toBe(true);
    expect(store.get(connectionAtom)).toMatchObject({ phase: "unauthorized", everOpen: true });
    await vi.advanceTimersByTimeAsync(30000);
    expect(FakeSocket.instances).toHaveLength(1);
    handle.dispose();
  });

  it("dispatches valid frames, sends only while open, and resyncs after atom updates", async () => {
    const store = createStore();
    const handle = connectPlanSocket(store, deps);
    const presence = vi.fn();
    const changed = { ...hello, checklist: { ...hello.checklist, current: { kind: "ready" } } };
    expect(changed.checklist.current).not.toEqual(hello.checklist.current);
    // Records what the callback sees instead of asserting inside it, so a
    // failure names the stale value rather than surfacing as a thrown frame.
    const resync = vi.fn(() => ({
      checklist: store.get(checklistAtom),
      epoch: store.get(connectionEpochAtom),
    }));
    const unsubscribe = handle.subscribe("presence", presence);
    const stopResync = handle.onResync(resync);
    expect(handle.send({ type: "reading", path: null })).toBe(false);
    await flush();
    latest().open();
    expect(handle.send({ type: "reading", path: null })).toBe(true);
    expect(latest().sent).toEqual(['{"type":"reading","path":null}']);
    latest().receive(hello);
    expect(resync).not.toHaveBeenCalled();
    expect(store.get(connectionEpochAtom)).toBe(0);
    expect(store.get(checklistAtom)).toEqual(hello.checklist);
    latest().receive({ type: "presence", entries: [] });
    expect(presence).toHaveBeenCalledTimes(1);
    expect(store.get(presenceAtom)).toEqual({ entries: [] });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    latest().receive({ type: "presence", entries: [], extra: true });
    expect(warn).toHaveBeenCalledOnce();
    expect(presence).toHaveBeenCalledTimes(1);
    latest().close();
    expect(handle.send({ type: "reading", path: null })).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    latest().open();
    latest().receive(changed);
    expect(resync).toHaveBeenCalledOnce();
    expect(resync).toHaveReturnedWith({ checklist: changed.checklist, epoch: 1 });
    unsubscribe();
    stopResync();
    latest().receive({ type: "presence", entries: [] });
    expect(presence).toHaveBeenCalledTimes(1);
    handle.dispose();
  });

  it("closes a socket that errors, retries once, and ignores later events from it", async () => {
    const store = createStore();
    const handle = connectPlanSocket(store, deps);
    await flush();
    const failed = latest();
    failed.open();
    failed.fail();
    expect(failed.readyState).toBe(3);
    expect(store.get(connectionAtom).phase).toBe("reconnecting");
    failed.fail();
    failed.close();
    await vi.advanceTimersByTimeAsync(999);
    expect(FakeSocket.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeSocket.instances).toHaveLength(2);
    expect(fetch).toHaveBeenCalledTimes(2);
    latest().open();
    expect(store.get(connectionAtom).phase).toBe("open");
    handle.dispose();
  });

  it("retries with growing delays when the WebSocket constructor throws", async () => {
    const store = createStore();
    const handle = connectPlanSocket(store, {
      WebSocketImpl: ThrowingSocket as unknown as typeof WebSocket,
    });
    await flush();
    expect(ThrowingSocket.attempts).toBe(1);
    expect(store.get(connectionAtom).phase).toBe("reconnecting");
    await vi.advanceTimersByTimeAsync(999);
    expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(ThrowingSocket.attempts).toBe(2);
    await vi.advanceTimersByTimeAsync(1999);
    expect(ThrowingSocket.attempts).toBe(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(ThrowingSocket.attempts).toBe(3);
    handle.dispose();
  });

  it("cancels a pending retry on dispose so no probe fires afterwards", async () => {
    const handle = connectPlanSocket(createStore(), deps);
    await flush();
    latest().close();
    expect(vi.getTimerCount()).toBe(1);
    handle.dispose();
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(30000);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("does not connect if disposed while a probe is pending", async () => {
    let resolve!: (response: Response) => void;
    vi.mocked(fetch).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const handle = connectPlanSocket(createStore(), deps);
    handle.dispose();
    resolve(response());
    await flush();
    expect(FakeSocket.instances).toHaveLength(0);
  });
});
