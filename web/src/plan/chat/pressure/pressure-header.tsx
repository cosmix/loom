import { useAtomValue, useStore } from "jotai";
import { useState } from "react";
import { Link } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { ChatSnapshot, SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView, PressureStepView } from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { authorBlockReasonAtom, isHolderAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { snapshotAtom } from "../model";
import { Untrusted, visibleControls } from "../visible-controls";
import { CodexStatusItem } from "./codex-status";
import { endLine, stepLabel } from "./steps";
import { pressureRunWaitingAtom, stopPressure, usePressureRun } from "./use-pressure";

const STOP_CONFIRM =
  "Stop the pressure test? The plan keeps the edits made so far, and no record is written.";

function Elapsed({ startMs, untilMs }: { startMs: number; untilMs: number }) {
  return <span>{formatElapsed(Math.max(0, Math.floor((untilMs - startMs) / 1000)))}</span>;
}

function LiveElapsed({ run }: { run: PressureRunView }) {
  const now = useNow();
  return <Elapsed startMs={run.started_ms} untilMs={now} />;
}

// The wire end has no timestamp. Use the final step's end, or the moment the end arrived.
function EndedElapsed({ run }: { run: PressureRunView }) {
  const [arrived] = useState(Date.now);
  return <Elapsed startMs={run.started_ms} untilMs={run.steps.at(-1)?.ended_ms ?? arrived} />;
}

function RunElapsed({ run }: { run: PressureRunView }) {
  return run.ended === null ? <LiveElapsed run={run} /> : <EndedElapsed run={run} />;
}

function editAddress(snapshot: ChatSnapshot, step: PressureStepView, path: string): string {
  if (!step.session_id) return toAddress(path);
  const frame =
    snapshot.session?.id === step.session_id
      ? snapshot
      : snapshot.previous?.session.id === step.session_id
        ? snapshot.previous
        : null;
  const turn = frame?.turns.findLast((item) =>
    item.changed_files.some((file) => file.path === path),
  );
  return turn && !turn.open
    ? openDiff("turn", path, `${step.session_id}/${turn.id}`)
    : toAddress(path);
}

function StepFooter({ step }: { step: PressureStepView }) {
  const snapshot = useAtomValue(snapshotAtom);
  const tokens = step.tokens;
  return (
    <div
      className="space-y-1"
      role="group"
      aria-label={`Round ${step.round} ${stepLabel(step.kind)}`}
    >
      {step.kind === "codex" && step.codex && (
        <CodexStatusItem status={step.codex} startedMs={step.started_ms} />
      )}
      <p>
        {stepLabel(step.kind)} · round {step.round}
      </p>
      {step.edits.map((path) => {
        const address = editAddress(snapshot, step, path);
        return (
          <div key={path} className="flex flex-wrap gap-x-2">
            <Link className="font-mono underline" to={toAddress(path)}>
              <Untrusted text={path} />
            </Link>
            {address !== toAddress(path) && (
              <Link className="underline" to={address}>
                Diff
              </Link>
            )}
          </div>
        );
      })}
      {tokens && (
        <p
          title={`Input: ${tokens.input}; cache write: ${tokens.cache_write}; cache read: ${tokens.cache_read}; output: ${tokens.output}`}
        >
          {compactTokens(tokens.input + tokens.cache_write + tokens.cache_read + tokens.output)}{" "}
          tokens
        </p>
      )}
    </div>
  );
}

function useStopRun() {
  const store = useStore();
  const holder = useAtomValue(isHolderAtom);
  const reason = useAtomValue(authorBlockReasonAtom);
  const server = useAtomValue(serverInfoAtom);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const writable = holder && server?.read_only.length === 0;
  async function stop() {
    if (stopping || !store.get(isHolderAtom) || store.get(authorBlockReasonAtom) !== null) return;
    if (store.get(serverInfoAtom)?.read_only.length !== 0) return;
    if (!window.confirm(STOP_CONFIRM)) return;
    setError(null);
    setStopping(true);
    const result = await stopPressure();
    // A stop the server took stays pending until the run's end frame replaces this control.
    if (result.ok) return;
    setStopping(false);
    setError(result.message);
  }
  return { writable, reason, stopping, error, stop };
}

function RunControls({ run }: { run: PressureRunView }) {
  const { writable, reason, stopping, error, stop } = useStopRun();
  const configChanged = run.ended?.reason?.startsWith("Codex changed session configuration");
  return (
    <>
      {run.ended && (
        <p>
          <Untrusted text={endLine(run.ended)} />
        </p>
      )}
      {configChanged && (
        <p className="whitespace-pre-wrap">
          <Untrusted text={run.ended?.reason ?? ""} />
        </p>
      )}
      {writable && !run.ended && (
        <DisabledReason
          reason={stopping ? "Stopping…" : reason}
          onClick={() => {
            void stop();
          }}
          size="sm"
        >
          Stop
        </DisabledReason>
      )}
      {error && <ActionError message={error} />}
    </>
  );
}

export function PressureHeader({ session }: { session: SessionView }) {
  const run = usePressureRun();
  const waiting = useAtomValue(pressureRunWaitingAtom);
  if (!run) return null;
  const fileName = visibleControls(run.plan.split("/").at(-1) ?? run.plan);
  return (
    <div className="space-y-2 px-3 text-xs text-muted-foreground" data-session-id={session.id}>
      <div className="chat-run-header flex flex-wrap gap-x-1">
        <span>
          Pressure test · {fileName} · round {run.round} of {run.rounds} · {stepLabel(run.step)}
        </span>
        <span>·</span>
        <RunElapsed key={run.started_ms} run={run} />
        <span>· {compactTokens(run.tokens_total)} tokens</span>
      </div>
      {waiting && <p role="status">reconnected; waiting for the run's next update</p>}
      {run.steps.map((step) => (
        <StepFooter key={`${step.round}/${step.kind}`} step={step} />
      ))}
      <RunControls key={run.started_ms} run={run} />
    </div>
  );
}
