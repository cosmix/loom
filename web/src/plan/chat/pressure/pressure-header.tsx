import { useAtomValue, useStore } from "jotai";
import { useEffect, useState } from "react";
import { Link } from "react-router";

import { formatElapsed } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { ChatSnapshot, SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView, PressureStepView } from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { authorBlockReasonAtom, isHolderAtom, serverInfoAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { snapshotAtom } from "../model";
import { CodexStatusItem } from "./codex-status";
import { endLine, stepLabel } from "./steps";
import {
  confirmConfig,
  pressureRunWaitingAtom,
  stopPressure,
  usePressureRun,
} from "./use-pressure";

const STOP_CONFIRM =
  "Stop the pressure test? The plan keeps the edits made so far, and no record is written.";

function RunElapsed({ run }: { run: PressureRunView }) {
  const [now, setNow] = useState(Date.now);
  const running = run.ended === null;
  useEffect(() => {
    if (!running) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [running]);
  // The wire end has no timestamp. Use the final step's end, or freeze the last tick.
  const until = running ? now : (run.steps.at(-1)?.ended_ms ?? now);
  return <span>{formatElapsed(Math.max(0, Math.floor((until - run.started_ms) / 1000)))}</span>;
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
    <div className="space-y-1" aria-label={`Round ${step.round} ${stepLabel(step.kind)}`}>
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
              {path}
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

function RunControls({ run }: { run: PressureRunView }) {
  const store = useStore();
  const holder = useAtomValue(isHolderAtom);
  const reason = useAtomValue(authorBlockReasonAtom);
  const server = useAtomValue(serverInfoAtom);
  const [error, setError] = useState<string | null>(null);
  const configChanged = run.ended?.reason?.startsWith("Codex changed session configuration");
  const writable = holder && server?.read_only.length === 0;
  async function act(confirm: boolean) {
    if (!store.get(isHolderAtom) || store.get(authorBlockReasonAtom) !== null) return;
    if (store.get(serverInfoAtom)?.read_only.length !== 0) return;
    if (confirm && !window.confirm(STOP_CONFIRM)) return;
    setError(null);
    const result = await (confirm ? stopPressure() : confirmConfig());
    if (!result.ok) setError(result.message);
  }
  return (
    <>
      {run.ended && <p>{endLine(run.ended)}</p>}
      {configChanged && <p className="whitespace-pre-wrap">{run.ended?.reason}</p>}
      {writable && (!run.ended || configChanged) && (
        <DisabledReason
          reason={reason}
          onClick={() => {
            void act(run.ended === null);
          }}
          size="sm"
        >
          {run.ended ? "I have checked these files" : "Stop"}
        </DisabledReason>
      )}
      {error && <p role="alert">{error}</p>}
    </>
  );
}

export function PressureHeader({ session }: { session: SessionView }) {
  const run = usePressureRun();
  const waiting = useAtomValue(pressureRunWaitingAtom);
  if (!run) return null;
  const fileName = run.plan.split("/").at(-1) ?? run.plan;
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
