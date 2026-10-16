import { createStore } from "jotai/vanilla";
import { afterEach, expect, it, vi } from "vitest";
import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { connectStatusSocket } from "@/api/ws";
import { connectionAtom, snapshotAtom } from "@/state/atoms";

class Socket {
  static instances: Socket[] = [];
  onopen?: () => void;
  onclose?: (event: { reason: string }) => void;
  onmessage?: (event: { data: string }) => void;
  constructor() {
    Socket.instances.push(this);
  }
  close() {
    this.onclose?.({ reason: "closed" });
  }
  receive(value: unknown) {
    this.onmessage?.({ data: JSON.stringify(value) });
  }
}

function setup() {
  vi.useFakeTimers();
  const store = createStore();
  let resolve!: (snapshot: unknown) => void;
  const response = new Promise((done) => {
    resolve = done;
  });
  const dispose = connectStatusSocket(store, {
    WebSocket: Socket as unknown as typeof WebSocket,
    fetch: (async () => ({ json: () => response })) as unknown as typeof fetch,
    location: { protocol: "http:", host: "test" },
  });
  return { store, resolve, dispose, socket: Socket.instances[0] };
}

const fixture = snapshotSchema.parse(fixtureJson);
const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
};
afterEach(() => {
  Socket.instances = [];
  vi.useRealTimers();
});

it("HTTP data never establishes live transport", async () => {
  const { store, resolve, dispose } = setup();
  resolve(fixture);
  await flush();
  expect(store.get(snapshotAtom)).toEqual(fixture);
  expect(store.get(connectionAtom).phase).toBe("connecting");
  dispose();
});

it.each(["disconnect", "reconnect", "dispose"])(
  "ignores delayed bootstrap after %s",
  async (operation) => {
    const { store, resolve, dispose, socket } = setup();
    socket.onopen?.();
    socket.receive(fixture);
    if (operation === "dispose") dispose();
    else socket.close();
    if (operation === "reconnect") {
      await vi.advanceTimersByTimeAsync(1000);
      Socket.instances[1].onopen?.();
    }
    const phase = store.get(connectionAtom);
    const newer = { ...fixture, generated_at: "2099-01-01T00:00:00Z" };
    resolve(newer);
    await flush();
    // Neither health nor data may be changed by a superseded continuation.
    expect(store.get(connectionAtom)).toEqual(phase);
    expect(store.get(snapshotAtom)).toEqual(fixture);
    socket.onopen?.();
    socket.receive(newer);
    expect(store.get(connectionAtom)).toEqual(phase);
    expect(store.get(snapshotAtom)).toEqual(fixture);
    dispose();
  },
);

it("an old HTTP snapshot cannot change health even when timestamp filtering rejects its data", async () => {
  const { store, resolve, dispose, socket } = setup();
  const newer = { ...fixture, generated_at: "2099-01-01T00:00:00Z" };
  socket.receive(newer);
  socket.close();
  resolve(fixture);
  await flush();
  expect(store.get(connectionAtom).phase).toBe("reconnecting");
  expect(store.get(snapshotAtom)).toEqual(newer);
  dispose();
});
