import { useAtomValue, useStore } from "jotai";
import { Square } from "lucide-react";
import { useState, type JSX } from "react";

import { compactTokens } from "@/lib/tokens";
import type { SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import { authorBlockReasonAtom, isHolderAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { Untrusted } from "../visible-controls";
import { Elapsed, LiveElapsed } from "./elapsed";
import { DoneHead, RunFileName } from "./done-head";
import { PressureDone } from "./pressure-done";
import { StepTimeline } from "./pressure-timeline";
import { endLine, runEndMs } from "./steps";
import { pressureRunWaitingAtom, stopPressure, usePressureRun } from "./use-pressure";

const STOP_CONFIRM =
  "Stop the pressure test? The plan keeps the edits made so far, and no record is written.";

function EndedElapsed({ run }: { run: PressureRunView }): JSX.Element {
  const [arrived] = useState(Date.now);
  return <Elapsed startMs={run.started_ms} untilMs={runEndMs(run, arrived)} />;
}

function RunElapsed({ run }: { run: PressureRunView }): JSX.Element {
  return run.ended === null ? <LiveElapsed startMs={run.started_ms} /> : <EndedElapsed run={run} />;
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

type StopRun = ReturnType<typeof useStopRun>;

function RunHead({ run, stop }: { run: PressureRunView; stop: StopRun }): JSX.Element {
  return (
    <div className="chat-run-header grid gap-2 px-4 py-3">
      <div className="flex flex-wrap items-center gap-2">
        {/* The head rests: the one turning roundel of the pane is the running step's. */}
        <ActivityGlyph kind="pressure" paused className="text-sm" />
        <span className="text-[13px] font-semibold text-foreground">Pressure test</span>
        <span className="inline-flex h-5 items-center rounded-md bg-muted px-1.5 text-xs font-medium">
          round {run.round} of {run.rounds}
        </span>
        {stop.writable && !run.ended && (
          <div className="ml-auto">
            <DisabledReason
              reason={stop.stopping ? "Stopping…" : stop.reason}
              onClick={() => {
                void stop.stop();
              }}
              size="sm"
            >
              <Square aria-hidden="true" className="fill-current text-(--tone-blocked)" />
              Stop
            </DisabledReason>
          </div>
        )}
      </div>
      <RunFileName plan={run.plan} />
      {/* No separator glyph: at a wrap, a dangling one would lead or trail a line. */}
      <div className="flex flex-wrap gap-x-3 font-mono text-xs text-muted-foreground">
        <RunElapsed run={run} />
        <span>{compactTokens(run.tokens_total)} tokens</span>
      </div>
    </div>
  );
}

function RunEnd({ run, error }: { run: PressureRunView; error: string | null }): JSX.Element {
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
      {error && <ActionError message={error} />}
    </>
  );
}

// Keyed by the run's start, so the stop state and the frozen clock reset with a new run.
function RunBody({ run, sessionId }: { run: PressureRunView; sessionId: string }): JSX.Element {
  const waiting = useAtomValue(pressureRunWaitingAtom);
  const stop = useStopRun();
  return (
    <div className="text-xs text-muted-foreground" data-session-id={sessionId}>
      {run.ended?.kind === "recorded" ? <DoneHead run={run} /> : <RunHead run={run} stop={stop} />}
      <div className="grid gap-3 px-4 py-3">
        {waiting && <p role="status">reconnected; waiting for the run's next update</p>}
        <PressureDone run={run} />
        <StepTimeline run={run} />
        <RunEnd run={run} error={stop.error} />
      </div>
    </div>
  );
}

export function PressureHeader({ session }: { session: SessionView }): JSX.Element | null {
  const run = usePressureRun();
  if (!run) return null;
  return <RunBody key={run.started_ms} run={run} sessionId={session.id} />;
}
