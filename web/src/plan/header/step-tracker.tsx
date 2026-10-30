import { useAtomValue } from "jotai/react";
import { Link } from "react-router";

import { cn } from "cn";
import type { ChecklistView, CurrentView, StepView } from "@/plan/api/schema";
import { visibleControls } from "@/plan/chat/visible-controls";
import { RailPopover, STEP_MARKS, StepGlyph, stepHref } from "@/plan/header/rail-popover";
import { runStripAtom } from "@/plan/run/use-run";

import "./step-rail.css";

const EXECUTE_STEP = 6;

export function isCurrentStep(current: CurrentView, step: StepView): boolean {
  return current.kind === "step" && current.number === step.number;
}

/** "Ready to run" / "All plans done" when no step is current, else null. */
export function readyLabel(current: CurrentView): string | null {
  if (current.kind === "ready") return "Ready to run";
  if (current.kind === "ready_all_done") return "All plans done";
  return null;
}

function StepCell({
  feature,
  step,
  current,
  live,
}: {
  feature: string;
  step: StepView;
  current: boolean;
  live: boolean;
}) {
  return (
    <li
      aria-current={current ? "step" : undefined}
      className={cn("plan-step-cell", current && "plan-step-cell-current")}
    >
      <Link to={stepHref(feature, step)} className="plan-step-link">
        <StepGlyph step={step} current={current} live={live} />
        <span className="plan-step-count tabular-nums" dir="ltr">
          {step.count === null ? "—" : visibleControls(step.count)}
        </span>
        <span className="plan-step-name" dir="ltr">
          {visibleControls(step.label)}
        </span>
        <span className="sr-only">, {STEP_MARKS[step.mark]}</span>
      </Link>
    </li>
  );
}

function summary(checklist: ChecklistView): string {
  const ready = readyLabel(checklist.current);
  if (ready) return ready;
  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  const total = checklist.steps.length;
  if (!step) return `${total} steps`;
  return `Step ${step.number} of ${total} · ${visibleControls(step.label)}`;
}

export function StepTracker({
  checklist,
  narrow,
}: {
  checklist: ChecklistView | null;
  narrow: boolean;
}) {
  const runStrip = useAtomValue(runStripAtom);
  if (checklist === null) return null;
  // The Run step shows the running ring only while a plan is actually executing.
  const liveNumber = runStrip?.daemon_running ? EXECUTE_STEP : null;
  if (narrow) {
    const currentNumber = checklist.current.kind === "step" ? checklist.current.number : null;
    return (
      <RailPopover
        checklist={checklist}
        summary={summary(checklist)}
        currentNumber={currentNumber}
        liveNumber={liveNumber}
      />
    );
  }
  const ready = readyLabel(checklist.current);
  return (
    <div className="flex min-w-0 max-w-full items-center gap-3">
      <ol aria-label="Steps" className="plan-step-rail">
        {checklist.steps.map((step) => (
          <StepCell
            key={step.number}
            feature={checklist.feature}
            step={step}
            current={isCurrentStep(checklist.current, step)}
            live={step.number === liveNumber}
          />
        ))}
      </ol>
      {ready && (
        <span className="shrink-0 text-xs font-medium whitespace-nowrap text-(--tone-completed)">
          {ready}
        </span>
      )}
    </div>
  );
}
