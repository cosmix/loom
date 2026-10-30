import { useAtomValue } from "jotai";
import { CircleDashed, X } from "lucide-react";
import type { JSX, ReactNode } from "react";

import { compactTokens } from "@/lib/tokens";
import type { ChatSnapshot } from "@/plan/api/schema-chat";
import type {
  PressureRunView,
  PressureStepKind,
  PressureStepView,
} from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { snapshotAtom } from "../model";
import { CodexStatusItem } from "./codex-status";
import { Elapsed, LiveElapsed } from "./elapsed";
import { FileChip } from "./file-chip";
import { stepLabel } from "./steps";

// The server runs the two reviews side by side, so Codex and /pressure come before /address.
const STEP_ORDER: readonly PressureStepKind[] = ["codex", "claude_pressure", "claude_address"];
const ROW_TEXT: Record<PressureStepKind, string> = {
  codex: "Codex reviews the plan",
  claude_pressure: "Claude reviews the plan",
  claude_address: "Claude addresses both reviews",
};
const RECORD_TEXT = "Write the record line";

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

function StepGlyph({ step }: { step: PressureStepView }): JSX.Element {
  if (step.state === "running") {
    const kind =
      step.kind === "codex" ? "codex" : step.kind === "claude_pressure" ? "pressure" : "drafting";
    return <ActivityGlyph kind={kind} className="text-sm" />;
  }
  const Glyph = step.state === "done" ? OTHER_GLYPHS.stepDone : X;
  const tone = step.state === "done" ? "text-(--tone-completed)" : "text-muted-foreground";
  return <Glyph className={`size-3.5 shrink-0 ${tone}`} aria-hidden="true" />;
}

function StepTime({ step }: { step: PressureStepView }): JSX.Element | null {
  if (step.state === "running") return <LiveElapsed startMs={step.started_ms} />;
  if (step.ended_ms === null) return null;
  return <Elapsed startMs={step.started_ms} untilMs={step.ended_ms} />;
}

function Row({
  glyph,
  text,
  state,
  time,
  children,
}: {
  glyph: ReactNode;
  text: string;
  state: string;
  time?: ReactNode;
  children?: ReactNode;
}): JSX.Element {
  return (
    <>
      <span className="flex justify-center">{glyph}</span>
      <span className="min-w-0">
        {text}
        <span className="sr-only">{state}</span>
      </span>
      <span className="font-mono text-[11.5px] text-muted-foreground">{time}</span>
      {children && (
        <div className="col-start-2 col-end-4 grid min-w-0 gap-1.5 pb-1">{children}</div>
      )}
    </>
  );
}

const ROW_GRID =
  "grid grid-cols-[20px_minmax(0,1fr)_auto] items-center gap-x-2 gap-y-1 text-[12.5px]";
// The server accepts 1 to 10 rounds (the form offers 1 to 3); the cap keeps a malformed count in
// a snapshot from queuing placeholder rows without end.
const MAX_QUEUED_ROUNDS = 10;

function StepRow({ step }: { step: PressureStepView }): JSX.Element {
  const snapshot = useAtomValue(snapshotAtom);
  const tokens = step.tokens;
  const hasExtras = step.edits.length > 0 || tokens !== null || step.codex !== null;
  return (
    <div
      role="group"
      aria-label={`Round ${step.round} ${stepLabel(step.kind)}`}
      className={ROW_GRID}
    >
      <Row
        glyph={<StepGlyph step={step} />}
        text={ROW_TEXT[step.kind]}
        state={step.state}
        time={<StepTime step={step} />}
      >
        {hasExtras && (
          <>
            {step.kind === "codex" && step.codex && (
              <CodexStatusItem status={step.codex} startedMs={step.started_ms} />
            )}
            {step.edits.map((path) => {
              const address = editAddress(snapshot, step, path);
              return (
                <FileChip
                  key={path}
                  path={path}
                  to={toAddress(path)}
                  diffTo={address !== toAddress(path) ? address : null}
                />
              );
            })}
            {tokens && (
              <p
                className="text-xs text-muted-foreground"
                title={`Input: ${tokens.input}; cache write: ${tokens.cache_write}; cache read: ${tokens.cache_read}; output: ${tokens.output}`}
              >
                {compactTokens(
                  tokens.input + tokens.cache_write + tokens.cache_read + tokens.output,
                )}{" "}
                tokens
              </p>
            )}
          </>
        )}
      </Row>
    </div>
  );
}

function QueuedRow({ text }: { text: string }): JSX.Element {
  return (
    <div className={`${ROW_GRID} text-muted-foreground`}>
      <Row
        glyph={<CircleDashed aria-hidden="true" className="size-3.5 shrink-0" />}
        text={text}
        state="queued"
      />
    </div>
  );
}

function RoundRows({
  round,
  steps,
  queue,
}: {
  round: number;
  steps: PressureStepView[];
  queue: boolean;
}): JSX.Element {
  return (
    <section data-pressure-round={round} className="grid gap-0.5">
      <p className="text-[10.5px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
        Round {round}
      </p>
      {STEP_ORDER.flatMap((kind) => {
        const present = steps.filter((step) => step.kind === kind);
        if (present.length > 0) {
          return present.map((step) => <StepRow key={`${round}/${kind}`} step={step} />);
        }
        return queue ? [<QueuedRow key={`${round}/${kind}`} text={ROW_TEXT[kind]} />] : [];
      })}
    </section>
  );
}

function RecordRow({ run }: { run: PressureRunView }): JSX.Element | null {
  if (run.ended !== null && run.ended.kind !== "recorded") return null;
  if (run.ended === null) return <QueuedRow text={RECORD_TEXT} />;
  return (
    <div className={ROW_GRID}>
      <Row
        glyph={
          <OTHER_GLYPHS.stepDone aria-hidden="true" className="size-3.5 text-(--tone-completed)" />
        }
        text={RECORD_TEXT}
        state="done"
      />
    </div>
  );
}

/**
 * Rounds in order, each with its steps. While the run is live, the current round's missing steps
 * and every later round show as queued rows, and a last row stands for the record line.
 */
export function StepTimeline({ run }: { run: PressureRunView }): JSX.Element {
  const byRound = new Map<number, PressureStepView[]>();
  for (const step of run.steps) byRound.set(step.round, [...(byRound.get(step.round) ?? []), step]);
  const live = run.ended === null;
  if (live) {
    for (let round = run.round; round <= Math.min(run.rounds, MAX_QUEUED_ROUNDS); round += 1) {
      if (!byRound.has(round)) byRound.set(round, []);
    }
  }
  return (
    <div data-pressure-timeline className="grid gap-2.5">
      {[...byRound]
        .sort(([left], [right]) => left - right)
        .map(([round, steps]) => (
          <RoundRows key={round} round={round} steps={steps} queue={live && round >= run.round} />
        ))}
      <RecordRow run={run} />
    </div>
  );
}
