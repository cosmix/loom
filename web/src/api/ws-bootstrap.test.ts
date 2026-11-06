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

const fixture = snapshotSchema.parse(fixtureJson);
const full = (snapshot: unknown) => ({ t: "full", snapshot });
const tick = (at: string) => ({ t: "tick", at, tick_age: 1, stages: {} });
const state = (at: string) => ({ t: "state", at, notice: "early" });

function setup() {
  vi.useFakeTimers();
  vi.spyOn(console, "error").mockImplementation(() => undefined);
  const request = vi.fn();
  vi.stubGlobal("fetch", request);
  const store = createStore();
  const dispose = connectStatusSocket(store, {
    WebSocket: Socket as unknown as typeof WebSocket,
    location: { protocol: "http:", host: "test" },
  });
  return { store, dispose, request, socket: Socket.instances[0] };
}

afterEach(() => {
  Socket.instances = [];
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("takes its first snapshot from the socket and requests nothing", () => {
  const { store, dispose, request, socket } = setup();

  expect(request).not.toHaveBeenCalled();
  expect(store.get(snapshotAtom)).toBeNull();
  socket.onopen?.();
  socket.receive(full(fixture));

  expect(store.get(snapshotAtom)).toEqual(fixture);
  expect(request).not.toHaveBeenCalled();
  dispose();
});

it("ignores a tick or state before the socket's first full", () => {
  const { store, dispose, socket } = setup();
  socket.onopen?.();

  socket.receive(tick("2099-01-01T00:00:00Z"));
  socket.receive(state("2099-01-01T00:00:00Z"));
  expect(store.get(snapshotAtom)).toBeNull();
  expect(store.get(connectionAtom).phase).toBe("live");

  socket.receive(full(fixture));
  socket.receive(tick("2099-01-01T00:00:01Z"));
  expect(store.get(snapshotAtom)?.generated_at).toBe("2099-01-01T00:00:01Z");
  dispose();
});

it("a reconnected socket must send its own full before deltas apply", async () => {
  const { store, dispose, socket } = setup();
  socket.onopen?.();
  socket.receive(full(fixture));
  socket.close();
  await vi.advanceTimersByTimeAsync(1000);
  const second = Socket.instances[1];
  second.onopen?.();

  second.receive(tick("2099-01-01T00:00:00Z"));
  expect(store.get(snapshotAtom)).toEqual(fixture);

  const newer = { ...fixture, generated_at: "2099-01-01T00:00:01Z", version: "newer" };
  second.receive(full(newer));
  expect(store.get(snapshotAtom)).toEqual(newer);
  dispose();
});

it.each(["disconnect", "reconnect", "dispose"])(
  "ignores a superseded socket's frames after %s",
  async (operation) => {
    const { store, dispose, socket } = setup();
    socket.onopen?.();
    socket.receive(full(fixture));
    if (operation === "dispose") dispose();
    else socket.close();
    if (operation === "reconnect") {
      await vi.advanceTimersByTimeAsync(1000);
      Socket.instances[1].onopen?.();
    }
    const phase = store.get(connectionAtom);

    socket.onopen?.();
    socket.receive(full({ ...fixture, generated_at: "2099-01-01T00:00:00Z" }));

    expect(store.get(connectionAtom)).toEqual(phase);
    expect(store.get(snapshotAtom)).toEqual(fixture);
    dispose();
  },
);
