import { atom } from "jotai/vanilla";

import { z } from "@/api/zod";
import { apiGet, apiPost, type ApiResult } from "@/plan/api/client";
import {
  runStateSchema,
  unmergedBranchSchema,
  type CommitRequest,
  type RunJob,
  type RunOutput,
  type RunStartRequest,
  type RunStateView,
  type RunStrip as RunSummary,
  type UnmergedBranch,
} from "@/plan/api/schema-run";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

export const runStateAtom = atom<RunStateView | null>(null);
export const runStripAtom = atom<RunSummary | null>(null);
export const runOutputAtom = atom<Record<RunJob, string[]>>({
  commit: [],
  start: [],
  abandon: [],
  finish_abandon: [],
  dashboard: [],
});

const acceptedSchema = z.strictObject({ accepted: z.literal(true) });
const renamedSchema = z.strictObject({ to: z.string() });
const unmergedSchema = z.strictObject({
  error: z.literal("unmerged_branches"),
  message: z.string(),
  unmerged_branches: z.array(unmergedBranchSchema),
});
const abandonResponseSchema = z.union([
  renamedSchema,
  unmergedSchema,
  z.strictObject({
    error: z.enum(["busy", "not_abandonable", "refused", "step_failed"]),
    message: z.string(),
  }),
]);
const dashboardSchema = z.strictObject({ url: z.string(), tunnel: z.string().optional() });

export type RunAccepted = z.infer<typeof acceptedSchema>;
export type AbandonResult = z.infer<typeof renamedSchema> | z.infer<typeof unmergedSchema>;
export type RunDashboard = z.infer<typeof dashboardSchema>;

export function appendRunOutput(store: Store, frame: RunOutput): void {
  const { job, line } = frame;
  // Completion frames carry no line; retain the job's output for inspection.
  if (line === null) return;
  store.set(runOutputAtom, (previous) => ({
    ...previous,
    [job]: [...previous[job], line].slice(-5000),
  }));
}

// Per-store state request generation: overlapping refetches (an action, a done frame and a
// resync) resolve in any order, so a response is applied only while it is the latest request and
// no run_state frame arrived since. While no state is known, any answer is applied: it beats
// knowing nothing, and the newer request replaces it on arrival. The server answers one request
// at a time, so waiting for the newest alone would hold a page on "checking" for every request
// queued ahead of it.
const stateRequests = new WeakMap<Store, number>();

function nextStateRequest(store: Store): number {
  const next = (stateRequests.get(store) ?? 0) + 1;
  stateRequests.set(store, next);
  return next;
}

export async function fetchRunState(store: Store): Promise<ApiResult<RunStateView>> {
  const current = nextStateRequest(store);
  const result = await apiGet("/api/run/state", runStateSchema);
  const known = store.get(runStateAtom) !== null;
  if (result.ok && (current === stateRequests.get(store) || !known)) {
    store.set(runStateAtom, result.value);
  }
  return result;
}

/** The server sends no run_state frames; state is refetched after every action that changes it. */
async function refetchAfter<T>(
  store: Store,
  request: Promise<ApiResult<T>>,
  changed: (value: T) => boolean = () => true,
): Promise<ApiResult<T>> {
  const result = await request;
  if (result.ok && changed(result.value)) void fetchRunState(store);
  return result;
}

// An unmerged_branches reply asks for confirmation and changes nothing.
const wasRenamed = (value: AbandonResult) => "to" in value;

export function commitRun(store: Store, body: CommitRequest): Promise<ApiResult<RunAccepted>> {
  return refetchAfter(
    store,
    apiPost("/api/run/commit", body, acceptedSchema, { contentType: "json" }),
  );
}

export function startRun(store: Store, body: RunStartRequest): Promise<ApiResult<RunAccepted>> {
  return refetchAfter(
    store,
    apiPost("/api/run/start", body, acceptedSchema, { contentType: "json" }),
  );
}

async function postAbandon(path: string, body: unknown): Promise<ApiResult<AbandonResult>> {
  const result = await apiPost(path, body, abandonResponseSchema, {
    contentType: "json",
    resultStatuses: [409],
  });
  if (!result.ok) return result;
  const value = result.value;
  // Only the branch-list conflict is a confirmation result; other 409s remain errors.
  if ("error" in value && value.error !== "unmerged_branches") {
    return { ok: false, status: 409, error: value.error, message: value.message };
  }
  return { ok: true, value };
}

export function abandonRun(
  store: Store,
  confirmed_unmerged: UnmergedBranch[],
): Promise<ApiResult<AbandonResult>> {
  return refetchAfter(store, postAbandon("/api/run/abandon", { confirmed_unmerged }), wasRenamed);
}

export function finishAbandon(store: Store): Promise<ApiResult<AbandonResult>> {
  return refetchAfter(store, postAbandon("/api/run/finish-abandon", {}), wasRenamed);
}

export function fetchDashboard(_store: Store): Promise<ApiResult<RunDashboard>> {
  // The route takes no body and its gate refuses any byte with 413, so post an empty text body.
  return apiPost("/api/run/dashboard", "", dashboardSchema, { contentType: "text" });
}

// Jobs whose output is streaming. runOutputAtom keeps a finished job's lines for inspection, so
// it cannot tell a second job of the same kind from a continuing one.
const streamingJobs = new WeakMap<Store, Set<RunJob>>();

/** Install once per store; follow socket replacement and refetch state after reconnects. */
export function installRunSubscription(store: Store): () => void {
  let detach = () => {};
  const attach = () => {
    detach();
    // A done frame may have been missed on the old socket; start tracking afresh.
    streamingJobs.get(store)?.clear();
    const socket = store.get(planSocketAtom);
    if (!socket) return;
    const stopState = socket.subscribe("run_state", ({ type: _type, ...state }) => {
      nextStateRequest(store);
      store.set(runStateAtom, state);
    });
    const stopOutput = socket.subscribe("run_output", ({ type: _type, ...output }) => {
      // No frame announces a job start; a frame from a job not yet streaming is the signal that it is busy.
      const streaming = streamingJobs.get(store) ?? new Set<RunJob>();
      streamingJobs.set(store, streaming);
      const starts = !streaming.has(output.job);
      if (output.done) streaming.delete(output.job);
      else streaming.add(output.job);
      appendRunOutput(store, output);
      if (output.done || starts) void fetchRunState(store);
    });
    const stopStrip = socket.subscribe("run_strip", ({ type: _type, ...strip }) => {
      const previous = store.get(runStripAtom);
      store.set(runStripAtom, strip);
      // The server pushes no run state, but the strip moves with it: a finished plan is renamed,
      // a run started from a terminal appears, a dead daemon stops. Progress counts alone do not
      // change the state, so they do not refetch; the first strip has nothing to differ from, and
      // whoever mounted a run view fetched on arrival.
      if (
        previous !== null &&
        (previous.plan !== strip.plan || previous.daemon_running !== strip.daemon_running)
      ) {
        void fetchRunState(store);
      }
    });
    const stopResync = socket.onResync(() => {
      // Frames were missed while disconnected, possibly a done frame.
      streamingJobs.get(store)?.clear();
      void fetchRunState(store);
    });
    detach = () => {
      stopState();
      stopOutput();
      stopStrip();
      stopResync();
    };
  };
  attach();
  const stopSocket = store.sub(planSocketAtom, attach);
  return () => {
    detach();
    stopSocket();
  };
}
