import { useSetAtom } from "jotai";
import { Gauge, ScanEye } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import { compactTokens } from "@/lib/tokens";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { plural } from "@/plan/files/heading-parts";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import { FileChip } from "./file-chip";
import { usePressureOptions } from "./use-pressure";

// DisabledReason fixes its button's look, so the ghost outline is laid over it from outside.
const GHOST_LOOK =
  "[&_button]:h-[30px] [&_button]:gap-1.5 [&_button]:rounded-lg [&_button]:border [&_button]:border-border [&_button]:bg-transparent [&_button]:px-3 [&_button]:text-[13px] [&_button]:font-medium [&_button]:text-foreground [&_button]:hover:bg-muted [&_button]:aria-disabled:hover:bg-transparent";

function EditedFiles({ paths }: { paths: string[] }): JSX.Element {
  return (
    <ul aria-label="Edited files" className="flex w-full flex-wrap gap-1.5">
      {paths.map((path) => (
        <li key={path} className="grid max-w-full">
          <FileChip path={path} to={toAddress(path)} diffTo={openDiff("head", path)} />
        </li>
      ))}
    </ul>
  );
}

function SummaryActions({ plan }: { plan: string }): JSX.Element {
  const options = usePressureOptions();
  const setDialog = useSetAtom(pressureDialogAtom);
  const nextPlan = options?.ok ? options.value.plans.find((path) => path !== plan) : undefined;
  // The review link and the next-plan control are direct children of the summary box.
  return (
    <>
      <Link
        className="inline-flex h-[30px] items-center gap-1.5 rounded-lg bg-primary px-3 text-[13px] font-semibold text-primary-foreground hover:bg-primary/80 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
        to={openDiff("head", plan)}
      >
        <ScanEye aria-hidden="true" className="size-3.5" />
        Review the changes
      </Link>
      <div className={GHOST_LOOK}>
        <DisabledReason
          reason={nextPlan === undefined ? "There is no other plan to pressure-test." : null}
          size="sm"
          onClick={() => {
            if (nextPlan !== undefined) setDialog({ open: true, plan: nextPlan });
          }}
        >
          <Gauge aria-hidden="true" />
          Test the next plan
        </DisabledReason>
      </div>
    </>
  );
}

export function PressureDone({ run }: { run: PressureRunView }): JSX.Element | null {
  return run.ended?.kind === "recorded" ? <RecordedSummary run={run} /> : null;
}

function RecordedSummary({ run }: { run: PressureRunView }): JSX.Element {
  const paths = [...new Set(run.steps.flatMap((step) => step.edits))];
  const addressed =
    run.rounds === 2 ? "Both rounds addressed" : `${plural(run.rounds, "round")} addressed`;
  return (
    <section role="region" aria-label="Pressure test summary" className="text-xs text-foreground">
      <div className="flex flex-wrap items-center gap-2 rounded-[10px] border border-[color-mix(in_oklch,var(--tone-completed)_45%,transparent)] bg-[color-mix(in_oklch,var(--tone-completed)_8%,transparent)] p-3.5">
        <div className="grid w-full gap-0.5">
          <p className="text-[13px] font-bold">{addressed}</p>
          <p className="text-muted-foreground">
            <span>{compactTokens(run.tokens_total)} tokens</span>
            <span> · {plural(paths.length, "file")} edited</span>
          </p>
        </div>
        <EditedFiles paths={paths} />
        <SummaryActions plan={run.plan} />
      </div>
    </section>
  );
}
