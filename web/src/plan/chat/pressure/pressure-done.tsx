import { useSetAtom } from "jotai";
import { useState, type JSX } from "react";
import { Link } from "react-router";

import { formatElapsed } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

import { Untrusted } from "../visible-controls";
import { runEndMs } from "./steps";
import { usePressureOptions } from "./use-pressure";

const DoneGlyph = OTHER_GLYPHS.stepDone;

function EditedFiles({ paths }: { paths: string[] }): JSX.Element {
  return (
    <ul aria-label="Edited files" className="space-y-1">
      {paths.map((path) => (
        <li key={path} className="flex flex-wrap items-baseline gap-x-3">
          <Link className="min-w-0 break-all font-mono underline" to={toAddress(path)}>
            <Untrusted text={path} />
          </Link>
          <Link className="underline" to={openDiff("head", path)}>
            Diff
          </Link>
        </li>
      ))}
    </ul>
  );
}

function SummaryActions({ plan }: { plan: string }): JSX.Element {
  const options = usePressureOptions();
  const setDialog = useSetAtom(pressureDialogAtom);
  const nextPlan = options?.ok ? options.value.plans.find((path) => path !== plan) : undefined;
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Link className="font-medium underline" to={openDiff("head", plan)}>
        Review the changes
      </Link>
      <DisabledReason
        reason={nextPlan === undefined ? "There is no other plan to pressure-test." : null}
        size="sm"
        onClick={() => {
          if (nextPlan !== undefined) setDialog({ open: true, plan: nextPlan });
        }}
      >
        Test the next plan
      </DisabledReason>
    </div>
  );
}

export function PressureDone({ run }: { run: PressureRunView }): JSX.Element | null {
  return run.ended?.kind === "recorded" ? <RecordedSummary run={run} /> : null;
}

// Mounted only once the recorded end arrives, so the arrival time matches the header's.
function RecordedSummary({ run }: { run: PressureRunView }): JSX.Element {
  const [arrivedMs] = useState(Date.now);
  const seconds = Math.max(0, Math.floor((runEndMs(run, arrivedMs) - run.started_ms) / 1000));
  const paths = [...new Set(run.steps.flatMap((step) => step.edits))];
  return (
    <section
      role="region"
      aria-label="Pressure test summary"
      className="space-y-3 rounded-lg border border-border bg-muted/30 p-3 text-xs text-foreground"
    >
      <div className="flex items-center gap-2 font-medium">
        <DoneGlyph aria-hidden="true" className="size-4 shrink-0 text-primary" />
        <span>Summary of this pressure test</span>
      </div>
      <p className="flex flex-wrap gap-x-3 text-muted-foreground">
        <span>
          {run.rounds} {run.rounds === 1 ? "round" : "rounds"}
        </span>
        <span>{formatElapsed(seconds)}</span>
        <span>{compactTokens(run.tokens_total)} tokens</span>
      </p>
      <EditedFiles paths={paths} />
      <SummaryActions plan={run.plan} />
    </section>
  );
}
