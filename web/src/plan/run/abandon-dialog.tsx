import { useAtomValue, useStore } from "jotai";
import { useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import type { AbandonView, RunStateView, UnmergedBranch } from "@/plan/api/schema-run";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom, isHolderAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";

import { RunOutput } from "./run-output";
import { abandonRun, finishAbandon, runOutputAtom } from "./use-run";

type Confirmation = { step: "first" | "branches"; branches: UnmergedBranch[] };

export function AbandonDialog({ state }: { state: RunStateView }) {
  const holder = useAtomValue(isHolderAtom);
  const server = useAtomValue(serverInfoAtom);
  if (state.abandon === null && state.finish_abandon === null) return null;
  const writable = holder && server !== null && server.read_only.length === 0;

  return (
    <>
      {state.abandon && writable && (
        <AbandonFlow key={state.abandon.plan} abandon={state.abandon} busy={state.busy !== null} />
      )}
      {state.finish_abandon !== null && (
        <div className="space-y-2">
          <p>This plan's run was removed but the plan was not retired</p>
          {writable && <FinishAbandon key={state.finish_abandon} busy={state.busy !== null} />}
        </div>
      )}
    </>
  );
}

function useAbandonFlow(abandon: AbandonView, busy: boolean) {
  const store = useStore();
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const inFlight = useRef(false);
  const reason = authorReason ?? (pending || busy ? "A run action is in progress" : null);
  async function submit(branches: UnmergedBranch[]) {
    if (reason !== null || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    const result = await abandonRun(store, branches);
    inFlight.current = false;
    setPending(false);
    if (!result.ok) setError(result.message);
    else if ("error" in result.value) {
      setConfirmation({ step: "branches", branches: result.value.unmerged_branches });
      setError(result.value.message);
    } else setConfirmation(null);
  }

  function confirm() {
    if (reason !== null || confirmation === null) return;
    if (confirmation.step === "first" && confirmation.branches.length > 0) {
      setConfirmation({ ...confirmation, step: "branches" });
    } else void submit(confirmation.step === "branches" ? confirmation.branches : []);
  }

  function begin() {
    setError(null);
    setCancelled(false);
    // Keep consent tied to this snapshot even if live run state changes behind the dialog.
    setConfirmation({
      step: "first",
      branches: abandon.unmerged_branches.map((item) => ({ ...item })),
    });
  }

  function cancel() {
    if (inFlight.current) return;
    setConfirmation(null);
    setError(null);
    setCancelled(true);
  }
  return { confirmation, pending, error, cancelled, reason, begin, cancel, confirm };
}

function AbandonFlow({ abandon, busy }: { abandon: AbandonView; busy: boolean }) {
  const flow = useAbandonFlow(abandon, busy);
  const trigger = useRef<HTMLDivElement>(null);
  return (
    <div className="space-y-2">
      <div ref={trigger}>
        <DisabledReason reason={flow.reason} onClick={flow.begin}>
          Abandon run
        </DisabledReason>
      </div>
      <p role="status">
        {flow.cancelled && "You can merge or push those branches first and come back."}
      </p>
      <Dialog
        open={flow.confirmation !== null}
        onOpenChange={(open) => {
          if (!open) flow.cancel();
        }}
      >
        <DialogContent
          key={flow.confirmation?.step}
          showCloseButton={!flow.pending}
          className="max-h-[85vh] overflow-y-auto sm:max-w-lg"
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            trigger.current?.querySelector("button")?.focus();
          }}
        >
          {flow.confirmation?.step === "first" && <FirstConfirmation abandon={abandon} />}
          {flow.confirmation?.step === "branches" && (
            <BranchConfirmation
              branches={flow.confirmation.branches}
              baseBranch={abandon.base_branch}
            />
          )}
          {flow.pending && <LoadState state="loading" title="Abandoning run" />}
          {(flow.pending || flow.error !== null) && <AbandonOutput />}
          {flow.error && <ActionError message={flow.error} />}
          <ConfirmationActions flow={flow} />
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** loom clean's output: what it left behind is the reason an abandon stopped (section 11). */
function AbandonOutput() {
  const hasLines = useAtomValue(runOutputAtom).abandon.length > 0;
  return hasLines ? <RunOutput job="abandon" /> : null;
}

function ConfirmationActions({ flow }: { flow: ReturnType<typeof useAbandonFlow> }) {
  return (
    <DialogFooter>
      <Button variant="outline" disabled={flow.pending} onClick={flow.cancel}>
        Cancel
      </Button>
      <DisabledReason reason={flow.reason} onClick={flow.confirm}>
        {flow.confirmation?.step === "branches"
          ? deleteLabel(flow.confirmation.branches.length)
          : "Abandon run"}
      </DisabledReason>
    </DialogFooter>
  );
}

function FirstConfirmation({ abandon }: { abandon: AbandonView }) {
  return (
    <>
      <DialogTitle>Abandon run</DialogTitle>
      <DialogDescription>
        <Untrusted text={`Abandon the run for ${abandon.plan}?`} />
      </DialogDescription>
      <ul className="list-disc space-y-1 pl-5">
        <li>
          The plan moves to doc/plans/archive/RETIRED-PLAN-&lt;slug&gt;.md and its metaplan entry
          closes.
        </li>
        <li>The daemon stops and every running stage session is killed.</li>
        <li>
          .loom/work is deleted, including stage state, memory journals, and the run's records.
        </li>
        <li>Every stage worktree under .worktrees/ and its loom/* branch is removed.</li>
      </ul>
      <StageList
        title={`Merged stages — work stays on ${abandon.base_branch}`}
        stages={abandon.merged_stages}
      />
      <StageList title="Unmerged stages" stages={abandon.unmerged_stages} />
    </>
  );
}

function StageList({ title, stages }: { title: string; stages: string[] }) {
  return (
    <section className="space-y-1">
      <h3 className="font-medium">
        <Untrusted text={title} />
      </h3>
      {stages.length === 0 ? (
        <p>None</p>
      ) : (
        <ul className="list-disc pl-5">
          {stages.map((stage) => (
            <li key={stage}>
              <Untrusted text={stage} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function deleteLabel(count: number) {
  return `Delete ${count} ${count === 1 ? "branch" : "branches"} with unmerged commits`;
}

function BranchConfirmation({
  branches,
  baseBranch,
}: {
  branches: UnmergedBranch[];
  baseBranch: string;
}) {
  return (
    <>
      <DialogTitle>Delete branches with unmerged commits?</DialogTitle>
      <DialogDescription>
        <Untrusted
          text={`These branches have commits ahead of ${baseBranch}. Abandoning the run deletes them.`}
        />
      </DialogDescription>
      <ul className="list-disc space-y-1 pl-5">
        {branches.map(({ branch, ahead }) => (
          <li key={branch}>
            <Untrusted
              text={`${branch}: ${ahead} ${ahead === 1 ? "commit" : "commits"} ahead of ${baseBranch}`}
            />
          </li>
        ))}
      </ul>
    </>
  );
}

function FinishAbandon({ busy }: { busy: boolean }) {
  const store = useStore();
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [finished, setFinished] = useState(false);
  const inFlight = useRef(false);
  const reason = authorReason ?? (pending || busy ? "A run action is in progress" : null);

  async function finish() {
    if (reason !== null || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    const result = await finishAbandon(store);
    inFlight.current = false;
    setPending(false);
    if (!result.ok) setError(result.message);
    else if ("error" in result.value) setError(result.value.message);
    else setFinished(true);
  }

  return (
    <>
      {!finished && (
        <DisabledReason
          reason={reason}
          onClick={() => {
            void finish();
          }}
        >
          Finish abandoning
        </DisabledReason>
      )}
      {pending && <LoadState state="loading" title="Finishing abandoning" />}
      {error && <ActionError message={error} />}
      <p role="status">{finished && "Plan retired"}</p>
    </>
  );
}
