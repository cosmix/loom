import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import stateFixture from "@/plan/api/fixtures/run-state.json";
import stripFixture from "@/plan/api/fixtures/run-strip.json";
import type { PlanMessage } from "@/plan/api/schema";
import { runStateSchema, runStripSchema, type RunOutput } from "@/plan/api/schema-run";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import { installRunSubscription, runOutputAtom, runStateAtom, runStripAtom } from "./use-run";

const initialState = () => runStateSchema.parse(stateFixture);
const output = (line: string | null, done = false): RunOutput => ({
  job: "start",
  step: "loom init",
  stream: line === null ? null : "stdout",
  line,
  exit_code: done ? 0 : null,
  done,
});
const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

function socketHarness(store: Store) {
  const listeners = new Set<(frame: PlanMessage) => void>();
  const resyncHandlers = new Set<() => void>();
  const socket: ReturnType<typeof connectPlanSocket> = {
    send: () => true,
    dispose: () => {
      listeners.clear();
      resyncHandlers.clear();
    },
    subscribe: (type, handler) => {
      const listener = (frame: PlanMessage) => {
        if (frame.type === type) {
          handler(frame as Extract<PlanMessage, { type: typeof type }>);
        }
      };
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    onResync: (handler) => {
      resyncHandlers.add(handler);
      return () => {
        resyncHandlers.delete(handler);
      };
    },
  };
  store.set(planSocketAtom, socket);
  return {
    receive: (frame: PlanMessage) => listeners.forEach((listener) => listener(frame)),
    resync: () => resyncHandlers.forEach((handler) => handler()),
  };
}

afterEach(() => vi.unstubAllGlobals());

describe("run subscription", () => {
  it("refetches state on resync after a missed run state frame", async () => {
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    socket.receive({ type: "run_state", ...initialState() });
    const fresh = { ...initialState(), workspace: "running", offer: "none_running", abandon: null };
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response(fresh));
    vi.stubGlobal("fetch", fetchMock);

    socket.resync();

    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/state",
      expect.objectContaining({
        method: "GET",
        credentials: "same-origin",
      }),
    );
    dispose();
  });

  it("refetches state when a job output frame reports completion", async () => {
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    const fresh = { ...initialState(), workspace: "running", offer: "none_running", abandon: null };
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(async () => response(fresh));
    vi.stubGlobal("fetch", fetchMock);
    socket.receive({ type: "run_output", ...output("running") });
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));

    socket.receive({ type: "run_output", ...output("more output") });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    socket.receive({ type: "run_output", ...output(null, true) });

    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/state",
      expect.objectContaining({ method: "GET" }),
    );
    dispose();
  });

  it("refetches state on the first output frame of a job", async () => {
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response(initialState()));
    vi.stubGlobal("fetch", fetchMock);

    socket.receive({ type: "run_output", ...output("first line") });

    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(initialState()));
    dispose();
  });

  it("refetches state at the first frame of each successive job of the same kind", async () => {
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => response(initialState()));
    vi.stubGlobal("fetch", fetchMock);

    socket.receive({ type: "run_output", ...output("job one first") });
    socket.receive({ type: "run_output", ...output("job one second") });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    socket.receive({ type: "run_output", ...output(null, true) });
    expect(fetchMock).toHaveBeenCalledTimes(2);

    // runOutputAtom still holds job one's lines; the next job is still a start.
    expect(store.get(runOutputAtom).start).toHaveLength(2);
    socket.receive({ type: "run_output", ...output("job two first") });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    socket.receive({ type: "run_output", ...output("job two second") });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    dispose();
  });

  it("treats the next frame after a resync as a job start when the done frame was missed", () => {
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => response(initialState()));
    vi.stubGlobal("fetch", fetchMock);

    socket.receive({ type: "run_output", ...output("job one first") });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // The disconnect swallowed job one's done frame; the resync refetches state.
    socket.resync();
    expect(fetchMock).toHaveBeenCalledTimes(2);

    socket.receive({ type: "run_output", ...output("job two first") });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    socket.receive({ type: "run_output", ...output("job two second") });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    dispose();
  });

  it("follows socket replacement and detaches all handlers on disposal", () => {
    const store = createStore();
    const dispose = installRunSubscription(store);
    const first = socketHarness(store);
    const strip = runStripSchema.parse(stripFixture);
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => response(initialState()));
    vi.stubGlobal("fetch", fetchMock);
    store.set(runOutputAtom, { ...store.get(runOutputAtom), start: ["earlier"] });
    first.receive({ type: "run_state", ...initialState() });
    const second = socketHarness(store);

    first.receive({ type: "run_state", ...initialState(), branch: "stale" });
    first.resync();
    second.receive({ type: "run_strip", ...strip });
    second.receive({ type: "run_output", ...output("new socket") });
    dispose();
    second.receive({ type: "run_state", ...initialState(), branch: "disposed" });
    second.receive({ type: "run_strip", ...strip, attention: 9 });
    second.receive({ type: "run_output", ...output("disposed") });
    second.resync();
    const third = socketHarness(store);
    third.receive({ type: "run_state", ...initialState(), branch: "after disposal" });
    third.resync();

    expect(store.get(runStateAtom)).toEqual(initialState());
    expect(store.get(runStripAtom)).toEqual(strip);
    expect(store.get(runOutputAtom).start).toEqual(["earlier", "new socket"]);
    // Only the live socket's "new socket" frame, a job start, refetches state.
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
