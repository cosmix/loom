import { useAtomValue, useStore } from "jotai";
import { X } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { ChatSnapshot, SessionView } from "@/plan/api/schema-chat";
import type {
  PressureRunView,
  PressureStepKind,
  PressureStepView,
} from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { authorBlockReasonAtom, isHolderAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { snapshotAtom } from "../model";
import { Untrusted } from "../visible-controls";
import { CodexStatusItem } from "./codex-status";
import { PressureDone } from "./pressure-done";
import { endLine, runEndMs, stepLabel } from "./steps";
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

function EndedElapsed({ run }: { run: PressureRunView }) {
  const [arrived] = useState(Date.now);
  return <Elapsed startMs={run.started_ms} untilMs={runEndMs(run, arrived)} />;
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

function StepGlyph({ step }: { step: PressureStepView }) {
  if (step.state === "running") {
    const kind =
      step.kind === "codex" ? "codex" : step.kind === "claude_pressure" ? "pressure" : "drafting";
    return <ActivityGlyph kind={kind} className="text-sm" />;
  }
  const Glyph = step.state === "done" ? OTHER_GLYPHS.stepDone : X;
  return <Glyph className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />;
}

function StepFooter({ step }: { step: PressureStepView }) {
  const snapshot = useAtomValue(snapshotAtom);
  const tokens = step.tokens;
  return (
    <div
      className="min-w-0 space-y-2 rounded-md border border-border/70 bg-background/60 p-3"
      role="group"
      aria-label={`Round ${step.round} ${stepLabel(step.kind)}`}
    >
      {step.kind === "codex" && step.codex && (
        <CodexStatusItem status={step.codex} startedMs={step.started_ms} />
      )}
      <p className="flex items-center gap-2">
        <StepGlyph step={step} />
        {stepLabel(step.kind)} · round {step.round}
        <span className="sr-only">{step.state}</span>
      </p>
      {step.edits.map((path) => {
        const address = editAddress(snapshot, step, path);
        return (
          <div key={path} className="flex flex-wrap gap-x-2 break-words">
            <Link className="min-w-0 break-words font-mono underline" to={toAddress(path)}>
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

function RoundRow({ round, steps }: { round: number; steps: PressureStepView[] }) {
  const reviewers: PressureStepKind[] = ["codex", "claude_pressure"];
  return (
    <div data-pressure-round={round} className="space-y-2">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,14rem),1fr))] gap-2">
        {reviewers.flatMap((kind) =>
          steps
            .filter((step) => step.kind === kind)
            .map((step) => <StepFooter key={`${round}/${step.kind}`} step={step} />),
        )}
      </div>
      {steps
        .filter((step) => step.kind === "claude_address")
        .map((step) => (
          <StepFooter key={`${round}/${step.kind}`} step={step} />
        ))}
    </div>
  );
}

function StepTimeline({ steps }: { steps: PressureStepView[] }) {
  const rounds = new Map<number, PressureStepView[]>();
  for (const step of steps) {
    const round = rounds.get(step.round) ?? [];
    round.push(step);
    rounds.set(step.round, round);
  }
  return (
    <div className="space-y-3">
      {[...rounds]
        .sort(([left], [right]) => left - right)
        .map(([round, items]) => (
          <RoundRow key={round} round={round} steps={items} />
        ))}
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
  const fileName = run.plan.split("/").at(-1) ?? run.plan;
  return (
    <div className="space-y-2 px-3 text-xs text-muted-foreground" data-session-id={session.id}>
      <div className="chat-run-header flex flex-wrap items-center gap-x-1">
        <ActivityGlyph kind="pressure" paused={run.ended !== null} className="mr-1 text-sm" />
        <Untrusted
          text={`Pressure test · ${fileName} · round ${run.round} of ${run.rounds} · ${stepLabel(run.step)}`}
        />
        <span>·</span>
        <RunElapsed key={run.started_ms} run={run} />
        <span>· {compactTokens(run.tokens_total)} tokens</span>
      </div>
      {waiting && <p role="status">reconnected; waiting for the run's next update</p>}
      <StepTimeline steps={run.steps} />
      <PressureDone run={run} />
      <RunControls key={run.started_ms} run={run} />
    </div>
  );
}
