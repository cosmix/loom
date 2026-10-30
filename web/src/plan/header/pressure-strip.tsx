import type { JSX } from "react";

import type { PressureRunView } from "@/plan/api/schema-pressure";
import { LiveElapsed } from "@/plan/chat/pressure/elapsed";
import { stepLabel } from "@/plan/chat/pressure/steps";
import { usePressureRun } from "@/plan/chat/pressure/use-pressure";
import { Untrusted } from "@/plan/chat/visible-controls";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";

/** Every round runs /pressure, Codex and /address once. */
const STEPS_PER_ROUND = 3;

function planTitle(path: string): string {
  return path.split("/").at(-1) ?? path;
}

function Progress({ run }: { run: PressureRunView }): JSX.Element {
  const total = Math.max(1, run.rounds * STEPS_PER_ROUND);
  const finished = Math.min(total, run.steps.filter((step) => step.state !== "running").length);
  return (
    <div
      role="progressbar"
      aria-label="Pressure test progress"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={finished}
      className="h-[5px] min-w-16 max-w-[260px] flex-1 overflow-hidden rounded-full bg-muted"
    >
      <div
        className="h-full rounded-full bg-(--tone-executing)"
        style={{ width: `${(finished / total) * 100}%` }}
      />
    </div>
  );
}

/** A full-width bar under the header while a pressure test runs; nothing otherwise. */
export function PressureStrip(): JSX.Element | null {
  const run = usePressureRun();
  if (run === null || run.ended !== null) return null;
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 border-b border-(--hairline) bg-[color-mix(in_oklch,var(--tone-executing)_8%,transparent)] px-5 py-2 text-[12.5px]">
      <span className="text-[16px] text-(--tone-executing)">
        <ActivityGlyph kind="pressure" />
      </span>
      <span className="min-w-0 font-semibold">
        Pressure-testing <Untrusted text={planTitle(run.plan)} />
      </span>
      <span className="text-muted-foreground">
        round {run.round} of {run.rounds} · {stepLabel(run.step)}
      </span>
      <Progress run={run} />
      <span className="font-mono text-[11.5px] text-muted-foreground tabular-nums">
        <LiveElapsed startMs={run.started_ms} />
      </span>
    </div>
  );
}
