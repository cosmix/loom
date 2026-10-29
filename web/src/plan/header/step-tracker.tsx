import { ChevronDownIcon } from "lucide-react";

import { cn } from "cn";
import { Button } from "@/components/ui/button";
import type { ChecklistView, CurrentView, MarkView, StepView } from "@/plan/api/schema";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { OTHER_GLYPHS, STEP_GLYPHS } from "@/plan/ui/glyphs";

import "./step-rail.css";

const MARKS: Record<MarkView, { label: string; tone: string }> = {
  done: { label: "done", tone: "text-(--tone-completed)" },
  gap: { label: "has gaps", tone: "text-(--tone-warning)" },
  active: { label: "in progress", tone: "text-(--tone-executing)" },
  optional: { label: "optional", tone: "text-(--tone-dimmed)" },
};

export function isCurrentStep(current: CurrentView, step: StepView): boolean {
  return current.kind === "step" && current.number === step.number;
}

/** "Ready to run" / "All plans done" when no step is current, else null. */
export function readyLabel(current: CurrentView): string | null {
  if (current.kind === "ready") return "Ready to run";
  if (current.kind === "ready_all_done") return "All plans done";
  return null;
}

function StepCell({ step, current }: { step: StepView; current: boolean }) {
  const mark = MARKS[step.mark];
  const Glyph = step.mark === "done" ? OTHER_GLYPHS.stepDone : STEP_GLYPHS[step.number - 1];
  return (
    <li
      aria-current={current ? "step" : undefined}
      className={cn(
        "plan-step-cell border-b-2",
        current
          ? "border-b-primary bg-muted text-foreground"
          : "border-b-transparent text-muted-foreground",
      )}
    >
      <span className="flex h-[18px] items-center gap-1.5">
        {Glyph && <Glyph className={cn("size-3.5 shrink-0", mark.tone)} aria-hidden="true" />}
        <span className="font-display text-lg leading-none font-semibold tabular-nums">
          {step.number}
        </span>
      </span>
      <span className="plan-step-name">{step.name}</span>
      <span className="sr-only">, {mark.label}</span>
    </li>
  );
}

/** Counter cells share their mark labels with the compact popover. */
export function StepList({
  checklist,
  className,
}: {
  checklist: ChecklistView;
  className?: string;
}) {
  return (
    <ol
      aria-label="Steps"
      className={cn(
        "plan-step-rail overflow-hidden rounded-lg border border-hairline bg-card/60 shadow-xs",
        className,
      )}
    >
      {checklist.steps.map((step) => (
        <StepCell key={step.number} step={step} current={isCurrentStep(checklist.current, step)} />
      ))}
    </ol>
  );
}

function summary(checklist: ChecklistView): string {
  const ready = readyLabel(checklist.current);
  if (ready) return ready;
  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  const total = checklist.steps.length;
  return step ? `Step ${step.number} of ${total} · ${step.name}` : `${total} steps`;
}

export function StepTracker({
  checklist,
  narrow,
}: {
  checklist: ChecklistView | null;
  narrow: boolean;
}) {
  if (checklist === null) return null;
  if (narrow) {
    return (
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="sm" className="min-w-0 max-w-full shrink text-xs">
            <span className="min-w-0 truncate">{summary(checklist)}</span>
            <ChevronDownIcon aria-hidden="true" />
          </Button>
        </PopoverTrigger>
        <PopoverContent
          data-loom-plan
          align="start"
          aria-label="Steps"
          className="max-h-(--radix-popover-content-available-height) overflow-y-auto"
        >
          <StepList checklist={checklist} className="plan-step-rail-vertical" />
        </PopoverContent>
      </Popover>
    );
  }
  const ready = readyLabel(checklist.current);
  return (
    <div className="flex min-w-0 max-w-full items-center gap-3 overflow-x-auto">
      <StepList checklist={checklist} />
      {ready && (
        <span className="shrink-0 text-xs font-medium whitespace-nowrap text-(--tone-completed)">
          {ready}
        </span>
      )}
    </div>
  );
}
