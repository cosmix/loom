import { useSetAtom } from "jotai";
import { Gauge, ScanEye } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { compactTokens } from "@/lib/tokens";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { plural } from "@/plan/ui/plural";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import { FileChip } from "./file-chip";
import { usePressureOptions } from "./use-pressure";

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
      <Button asChild data-look="secondary" size="sm" className="plan-button">
        <Link to={openDiff("head", plan)}>
          <ScanEye aria-hidden="true" />
          Review the changes
        </Link>
      </Button>
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
