import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import stateFixture from "@/plan/api/fixtures/run-state.json";
import type { PlanMessage } from "@/plan/api/schema";
import { runStateSchema, type RunOutput, type UnmergedBranch } from "@/plan/api/schema-run";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import {
  abandonRun,
  appendRunOutput,
  commitRun,
  fetchDashboard,
  fetchRunState,
  finishAbandon,
  installRunSubscription,
  runOutputAtom,
  runStateAtom,
  startRun,
} from "./use-run";

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

describe("run output", () => {
  it("caps each job at 5000 lines and drops the oldest", () => {
    const store = createStore();
    store.set(runOutputAtom, {
      ...store.get(runOutputAtom),
      start: Array.from({ length: 5000 }, (_, index) => `line ${index}`),
      commit: ["commit output"],
    });

    appendRunOutput(store, output("newest"));

    const lines = store.get(runOutputAtom);
    expect(lines.start).toHaveLength(5000);
    expect(lines.start[0]).toBe("line 1");
    expect(lines.start.at(-1)).toBe("newest");
    expect(lines.commit).toEqual(["commit output"]);
  });

  it("keeps blank lines and retains output on a completion frame", () => {
    const store = createStore();
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(response(initialState())));
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    socket.receive({ type: "run_output", ...output("Workspace initialized") });
    socket.receive({ type: "run_output", ...output("") });

    socket.receive({ type: "run_output", ...output(null, true) });

    expect(store.get(runOutputAtom).start).toEqual(["Workspace initialized", ""]);
    dispose();
  });
});

describe("run requests", () => {
  it.each<{ branches: UnmergedBranch[] }>([
    { branches: [] },
    {
      branches: [
        { branch: "loom/implementation", ahead: 2 },
        { branch: "loom/tests", ahead: 7 },
      ],
    },
  ])("posts exactly the confirmed unmerged branch list %#", async ({ branches }) => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValue(response({ to: "doc/plans/RETIRED-example.md" }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await abandonRun(createStore(), branches);

    expect(result).toEqual({ ok: true, value: { to: "doc/plans/RETIRED-example.md" } });
    expect(fetchMock).toHaveBeenCalledWith("/api/run/abandon", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ confirmed_unmerged: branches }),
    });
  });

  it("returns the fresh branch list from an unmerged branches conflict", async () => {
    const conflict = {
      error: "unmerged_branches",
      message: "Confirm the new list",
      unmerged_branches: [{ branch: "loom/new-stage", ahead: 3 }],
    };
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(response(conflict, 409)));

    const result = await abandonRun(createStore(), []);

    expect(result).toEqual({ ok: true, value: conflict });
  });

  it("preserves ordinary abandon conflicts as API errors", async () => {
    const refusal = { error: "step_failed", message: "loom stop exited with code 1" };
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(response(refusal, 409)));

    const result = await abandonRun(createStore(), []);

    expect(result).toEqual({ ok: false, status: 409, ...refusal });
  });

  it("validates fresh state before replacing the atom", async () => {
    const store = createStore();
    store.set(runStateAtom, initialState());
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockResolvedValue(response({ workspace: "absent" })),
    );

    const result = await fetchRunState(store);

    expect(result).toMatchObject({ ok: false, error: "bad_response" });
    expect(store.get(runStateAtom)).toEqual(initialState());
  });

  it("parses accepted commit and start responses", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => response({ accepted: true }, 202));
    vi.stubGlobal("fetch", fetchMock);
    const store = createStore();
    const commit = { paths: ["doc/plans/PLAN-example.md"], message: "docs(plan): example" };
    const start = { plan: "doc/plans/PLAN-example.md", backend: "tmux", clean: false } as const;

    expect(await commitRun(store, commit)).toEqual({ ok: true, value: { accepted: true } });
    expect(await startRun(store, start)).toEqual({ ok: true, value: { accepted: true } });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/commit",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(commit),
      }),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/start",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(start),
      }),
    );
  });

  it("posts a JSON body to finish abandoning and parses the renamed path", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValue(response({ to: "doc/plans/RETIRED-example.md" }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await finishAbandon(createStore());

    expect(result).toEqual({ ok: true, value: { to: "doc/plans/RETIRED-example.md" } });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/finish-abandon",
      expect.objectContaining({
        method: "POST",
        body: "{}",
      }),
    );
  });

  it.each([
    { url: "http://127.0.0.1:8080/" },
    { url: "http://127.0.0.1:8080/", tunnel: "ssh -L 8080:localhost:8080 host" },
  ])("parses dashboard responses with an optional tunnel %#", async (dashboard) => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response(dashboard));
    vi.stubGlobal("fetch", fetchMock);

    const result = await fetchDashboard(createStore());

    expect(result).toEqual({ ok: true, value: dashboard });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/dashboard",
      expect.objectContaining({ method: "POST" }),
    );
  });
});

describe("dashboard request", () => {
  it("posts an empty body: the route's gate refuses any byte with 413", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => response({ url: "http://127.0.0.1:8080/" }));
    vi.stubGlobal("fetch", fetchMock);

    await fetchDashboard(createStore());

    const init = fetchMock.mock.calls[0]![1]!;
    expect(init.body).toBe("");
    expect(new Headers(init.headers).get("Content-Type")).toMatch(/^text\/plain/);
  });
});

describe("overlapping state fetches", () => {
  function deferred() {
    let resolve!: (value: Response) => void;
    const promise = new Promise<Response>((done) => {
      resolve = done;
    });
    return { promise, resolve };
  }

  it("applies only the latest request's response, whatever the arrival order", async () => {
    const older = { ...initialState(), workspace: "absent", offer: "start" } as const;
    const newer = { ...initialState(), workspace: "running", offer: "none_running" } as const;
    const first = deferred();
    const second = deferred();
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise),
    );
    const store = createStore();

    const olderFetch = fetchRunState(store);
    const newerFetch = fetchRunState(store);
    second.resolve(response(newer));
    await newerFetch;
    first.resolve(response(older));
    const stale = await olderFetch;

    expect(stale).toMatchObject({ ok: true });
    expect(store.get(runStateAtom)).toEqual(newer);
  });

  it("drops a response that a run_state frame overtook", async () => {
    const pending = deferred();
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockReturnValueOnce(pending.promise));
    const store = createStore();
    const socket = socketHarness(store);
    const dispose = installRunSubscription(store);
    const frame = { ...initialState(), workspace: "running", offer: "none_running" } as const;

    const request = fetchRunState(store);
    socket.receive({ type: "run_state", ...frame });
    pending.resolve(response({ ...initialState(), workspace: "absent", offer: "start" }));
    await request;

    expect(store.get(runStateAtom)).toEqual(frame);
    dispose();
  });
});

describe("run state refresh after actions", () => {
  const fresh = () => ({ ...initialState(), workspace: "running", offer: "none_running" });
  const route = (accepted: unknown, status = 200) =>
    vi.fn<typeof fetch>(async (input) =>
      String(input) === "/api/run/state" ? response(fresh()) : response(accepted, status),
    );

  it("refetches state after an accepted commit", async () => {
    const store = createStore();
    const fetchMock = route({ accepted: true }, 202);
    vi.stubGlobal("fetch", fetchMock);

    await commitRun(store, { paths: ["doc/plans/PLAN-example.md"], message: "docs: example" });

    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh()));
    expect(fetchMock).toHaveBeenCalledWith("/api/run/state", expect.anything());
  });

  it("refetches state after an accepted start", async () => {
    const store = createStore();
    vi.stubGlobal("fetch", route({ accepted: true }, 202));

    await startRun(store, { plan: "doc/plans/PLAN-example.md", backend: "tmux", clean: false });

    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh()));
  });

  it("does not refetch after a rejected commit", async () => {
    const store = createStore();
    const fetchMock = route({ error: "busy", message: "A job is running" }, 409);
    vi.stubGlobal("fetch", fetchMock);

    const result = await commitRun(store, { paths: [], message: "x" });

    expect(result.ok).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("refetches state after a successful abandon and finish-abandon", async () => {
    const store = createStore();
    const fetchMock = route({ to: "doc/plans/RETIRED-example.md" });
    vi.stubGlobal("fetch", fetchMock);

    await abandonRun(store, []);
    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh()));
    store.set(runStateAtom, null);
    await finishAbandon(store);

    await vi.waitFor(() => expect(store.get(runStateAtom)).toEqual(fresh()));
  });
});
