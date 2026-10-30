import { CircleCheck } from "lucide-react";
import { useState, type JSX } from "react";

import { formatElapsed } from "@/lib/format";
import { plural } from "@/plan/files/heading-parts";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import { Untrusted } from "../visible-controls";
import { runEndMs } from "./steps";

/** The plan's file name on a line of its own, under a run head's top row. */
export function RunFileName({ plan }: { plan: string }): JSX.Element {
  return (
    <div className="font-mono text-[11.5px] break-all">
      <Untrusted text={plan.split("/").at(-1) ?? plan} />
    </div>
  );
}

/** Seconds from the start to the moment the recorded end arrived; the clock freezes there. */
function useRecordedSeconds(run: PressureRunView): number {
  const [arrivedMs] = useState(Date.now);
  return Math.max(0, Math.floor((runEndMs(run, arrivedMs) - run.started_ms) / 1000));
}

/** The run head once the record line is written: it replaces the live head at the top. */
export function DoneHead({ run }: { run: PressureRunView }): JSX.Element {
  const seconds = useRecordedSeconds(run);
  return (
    <div className="chat-run-header grid gap-2 border-b border-hairline px-4 py-3">
      <div className="flex flex-wrap items-center gap-2">
        <CircleCheck aria-hidden="true" className="size-4 shrink-0 text-(--tone-completed)" />
        <span className="text-[13.5px] font-bold text-foreground">Pressure test finished</span>
        <span className="inline-flex h-5 items-center gap-1 rounded-md bg-muted px-1.5 text-[11.5px] font-medium">
          <span>{plural(run.rounds, "round")}</span>
          <span aria-hidden="true">·</span>
          <span>{formatElapsed(seconds)}</span>
        </span>
      </div>
      <RunFileName plan={run.plan} />
    </div>
  );
}
