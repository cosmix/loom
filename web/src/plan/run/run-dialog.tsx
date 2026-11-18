import { useAtom, useAtomValue, useStore } from "jotai";
import { useEffect, useRef, useState, type JSX } from "react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import type { ApiResult } from "@/plan/api/client";
import { runJobSchema, type Backend, type RunJob, type RunStateView } from "@/plan/api/schema-run";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { ActionError } from "@/plan/ui/action-error";
import { LoadState } from "@/plan/ui/load-state";

import { AbandonDialog } from "./abandon-dialog";
import { DashboardButton } from "./dashboard-button";
import { RunOutput } from "./run-output";
import { commitRun, fetchRunState, runOutputAtom, runStateAtom, startRun } from "./use-run";

export function RunDialog(): JSX.Element | null {
  const store = useStore();
  const [dialog, setDialog] = useAtom(runDialogAtom);
  const state = useAtomValue(runStateAtom);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!dialog.open) return;
    let active = true;
    setError(null);
    void fetchRunState(store).then((result) => {
      if (active && !result.ok) setError(result.message);
    });
    return () => {
      active = false;
    };
  }, [store, dialog.open]);

  if (!dialog.open) return null;
  const plan = dialog.plan ?? state?.preselected ?? state?.plans[0]?.path ?? "";
  return (
    <Dialog open onOpenChange={(open) => setDialog({ ...dialog, open })}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-xl">
        <DialogTitle>Start a run</DialogTitle>
        <DialogDescription>
          Choose a plan and prepare the workspace for execution.
        </DialogDescription>
        {error && <ActionError message={error} />}
        {state ? (
          <RunForm key={plan} state={state} initialPlan={plan} />
        ) : (
          !error && <LoadState state="loading" size="compact" title="Loading run state" />
        )}
      </DialogContent>
    </Dialog>
  );
}

function useRunRequest() {
  const [pending, setPending] = useState<RunJob | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A synchronous guard: two clicks in one task both see the render before `pending` is set.
  const inFlight = useRef(false);
  async function submit(job: RunJob, request: () => Promise<ApiResult<unknown>>) {
    if (inFlight.current) return;
    inFlight.current = true;
    setPending(job);
    setError(null);
    const result = await request();
    inFlight.current = false;
    if (!result.ok) setError(result.message);
    setPending(null);
  }
  return { pending, error, submit };
}

/**
 * Commit and Start are holder-only on the server. While nobody holds authoring, authorReason is
 * null but a browser that has not taken it would get a 403.
 */
function useWriteReason(): string | null {
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const holder = useAtomValue(isHolderAtom);
  return (
    authorReason ?? (holder ? null : "Start a chat to take authoring, then commit or start a run.")
  );
}

function useRunForm(state: RunStateView, initialPlan: string) {
  const store = useStore();
  const writeReason = useWriteReason();
  const [plan, setPlan] = useState(initialPlan);
  const [choice, setBackend] = useState<Backend>(state.default_backend);
  const request = useRunRequest();
  const busy = state.busy ?? request.pending;
  const clean =
    state.clean_notice ||
    (state.workspace === "initialized" && state.active_plan !== null && plan !== state.active_plan);
  const backend = state.backends.includes(choice) ? choice : state.backends[0];
  const busyReason = busy ? `Waiting for ${busy} to finish` : null;
  const commitReason = writeReason ?? busyReason;
  const startReason =
    writeReason ??
    changesReason(state) ??
    busyReason ??
    (!plan ? "Choose a plan first" : !backend ? "No backend is available" : null);
  const commit = (paths: string[], message: string) => {
    void request.submit("commit", () => commitRun(store, { paths, message }));
  };
  const start = () => {
    if (backend) void request.submit("start", () => startRun(store, { plan, backend, clean }));
  };
  return {
    plan,
    setPlan,
    backend,
    setBackend,
    request,
    busy,
    clean,
    commitReason,
    startReason,
    commit,
    start,
  };
}

function changesReason(state: RunStateView): string | null {
  if (state.blocking_changes.length) return "Commit or stash the blocking changes first";
  if (state.doc_changes.length) return "Commit the changes under doc/ first";
  return null;
}

function RunForm({ state, initialPlan }: { state: RunStateView; initialPlan: string }) {
  const form = useRunForm(state, initialPlan);
  return (
    <div className="space-y-4">
      <PlanSelect state={state} plan={form.plan} onChange={form.setPlan} />
      <p>
        <Untrusted text={`merges into ${state.branch}`} />
      </p>
      <DocChanges
        key={form.plan}
        state={state}
        plan={form.plan}
        reason={form.commitReason}
        onCommit={form.commit}
      />
      <BlockingChanges state={state} />
      <BackendSelect backends={state.backends} backend={form.backend} onChange={form.setBackend} />
      {form.clean && (
        <p>
          Starting this plan uses --clean: stops the old daemon, reaps its sessions, archives its
          memory, and removes its worktrees and workspace.
        </p>
      )}
      {form.request.pending && (
        <LoadState state="loading" size="compact" title={`Submitting ${form.request.pending}`} />
      )}
      {form.request.error && <ActionError message={form.request.error} />}
      <RunActions state={state} form={form} />
      <JobOutputs busy={form.busy} />
    </div>
  );
}

function RunActions({ state, form }: { state: RunStateView; form: ReturnType<typeof useRunForm> }) {
  return (
    <div className="flex flex-wrap items-start gap-3">
      {state.offer === "none_running" ? (
        <p>The plan is running</p>
      ) : (
        <DisabledReason reason={form.startReason} look="primary" onClick={form.start}>
          {primaryLabel(state.offer)}
        </DisabledReason>
      )}
      {(state.offer === "none_running" || state.offer === "resume" || state.dashboard_url) && (
        <DashboardButton />
      )}
      {(state.abandon || state.finish_abandon) && <AbandonDialog state={state} />}
    </div>
  );
}

function primaryLabel(offer: RunStateView["offer"]): string {
  return {
    start: "Start",
    start_initialized: "Start the run",
    resume: "Resume",
    start_next: "Start the next plan",
    none_running: "",
  }[offer];
}

function PlanSelect({
  state,
  plan,
  onChange,
}: {
  state: RunStateView;
  plan: string;
  onChange: (plan: string) => void;
}) {
  const choices = [...state.plans].sort(
    (a, b) => Number(b.path === plan) - Number(a.path === plan),
  );
  return (
    <label className="block space-y-1">
      Plan
      <select
        className="block w-full rounded border bg-background p-2"
        value={plan}
        onChange={(event) => onChange(event.target.value)}
      >
        {!choices.some((choice) => choice.path === plan) && (
          <option value={plan}>{plan ? visibleControls(plan) : "Choose a plan"}</option>
        )}
        {choices.map((choice) => (
          <option key={choice.path} value={choice.path}>
            {visibleControls(choice.path)}
            {!choice.listed && " (unlisted)"}
          </option>
        ))}
      </select>
    </label>
  );
}

type DocChangesProps = {
  state: RunStateView;
  plan: string;
  reason: string | null;
  onCommit: (paths: string[], message: string) => void;
};

function DocChanges({ state, plan, reason, onCommit }: DocChangesProps) {
  const [unchecked, setUnchecked] = useState<Set<string>>(() => new Set());
  const [message, setMessage] = useState(
    `docs(plan): planning documents for ${plan.split("/").at(-1) ?? plan}`,
  );
  const paths = state.doc_changes
    .filter(({ path }) => !unchecked.has(path))
    .map(({ path }) => path);
  const commitReason =
    reason ??
    (paths.length === 0
      ? "Select changes to commit"
      : !message.trim()
        ? "Enter a commit message"
        : null);
  const toggle = (path: string, checked: boolean) =>
    setUnchecked((previous) => {
      const next = new Set(previous);
      if (checked) next.delete(path);
      else next.add(path);
      return next;
    });
  if (state.doc_changes.length === 0) return null;
  return (
    <section className="space-y-2" aria-label="Changes under doc/">
      <h3>Uncommitted changes under doc/</h3>
      <DocPaths changes={state.doc_changes} unchecked={unchecked} toggle={toggle} />
      <label className="block">
        Commit message
        <input
          className="block w-full rounded border bg-background p-2"
          value={message}
          onChange={(event) => setMessage(event.target.value)}
        />
      </label>
      <DisabledReason reason={commitReason} onClick={() => onCommit(paths, message)}>
        Commit changes under doc/
      </DisabledReason>
    </section>
  );
}

function DocPaths({
  changes,
  unchecked,
  toggle,
}: {
  changes: RunStateView["doc_changes"];
  unchecked: Set<string>;
  toggle: (path: string, checked: boolean) => void;
}) {
  return (
    <>
      {changes.map(({ path, status }) => (
        <label key={path} className="flex gap-2">
          <input
            type="checkbox"
            checked={!unchecked.has(path)}
            onChange={(event) => toggle(path, event.target.checked)}
          />
          <span>
            <Untrusted text={`${path} (${status})`} />
          </span>
        </label>
      ))}
    </>
  );
}

function BlockingChanges({ state }: { state: RunStateView }) {
  if (state.blocking_changes.length === 0) return null;
  return (
    <section aria-label="Blocking changes">
      <h3>Blocking changes</h3>
      <ul>
        {state.blocking_changes.map(({ path, status }) => (
          <li key={path}>
            <Untrusted text={`${path} (${status})`} />
          </li>
        ))}
      </ul>
      <p>Commit or stash these first; the app does not touch files outside doc/.</p>
    </section>
  );
}

function BackendSelect({
  backends,
  backend,
  onChange,
}: {
  backends: Backend[];
  backend: Backend | undefined;
  onChange: (backend: Backend) => void;
}) {
  return (
    <fieldset className="space-y-1">
      <legend>Backend</legend>
      {backends.map((choice) => (
        <label key={choice} className="mr-4 inline-flex gap-2">
          <input
            type="radio"
            name="run-backend"
            value={choice}
            checked={backend === choice}
            onChange={() => onChange(choice)}
          />
          {choice === "native" ? "Native" : "tmux"}
        </label>
      ))}
    </fieldset>
  );
}

function JobOutputs({ busy }: { busy: RunJob | null }) {
  const output = useAtomValue(runOutputAtom);
  return (
    <>
      {runJobSchema.options
        .filter((job) => job === busy || output[job].length > 0)
        .map((job) => (
          <RunOutput key={job} job={job} />
        ))}
    </>
  );
}
