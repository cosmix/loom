import { ChevronDownIcon } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";

import { cn } from "cn";
import { Button } from "@/components/ui/button";
import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import { visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { OTHER_GLYPHS, STEP_GLYPHS } from "@/plan/ui/glyphs";

/** Screen-reader wording for each step mark, shared by the rail cells and this list. */
export const STEP_MARKS: Record<MarkView, string> = {
  done: "done",
  gap: "has gaps",
  active: "in progress",
  optional: "optional",
};

export function stepHref(feature: string, step: StepView): string {
  return `/f/${encodeURIComponent(feature)}?step=${step.number}`;
}

/**
 * The glyph of one step: the circle-check once done, the running ring while the Run step executes,
 * else the step's own glyph (full-strength when it is the current step).
 */
export function StepGlyph({
  step,
  current,
  live,
}: {
  step: StepView;
  current: boolean;
  live: boolean;
}) {
  if (live) {
    return <ActivityGlyph kind="running" className="plan-step-glyph text-(--tone-executing)" />;
  }
  const Glyph = step.mark === "done" ? OTHER_GLYPHS.stepDone : STEP_GLYPHS[step.number - 1];
  if (!Glyph) return null;
  const tone =
    step.mark === "done"
      ? "text-(--tone-completed)"
      : current
        ? "text-foreground"
        : "text-muted-foreground";
  return <Glyph className={cn("plan-step-glyph", tone)} aria-hidden="true" />;
}

function StepListText({ step }: { step: StepView }) {
  return (
    <>
      <span dir="ltr">{visibleControls(step.label)}</span>
      <span className="plan-step-list-count tabular-nums" dir="ltr">
        {step.count === null ? "—" : visibleControls(step.count)}
      </span>
    </>
  );
}

/** The narrow rail: one summary button whose popover lists the steps as links. */
export function RailPopover({
  checklist,
  summary,
  currentNumber,
  liveNumber,
}: {
  checklist: ChecklistView;
  summary: string;
  currentNumber: number | null;
  liveNumber: number | null;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="plan-rail-summary text-xs">
          <span dir="ltr">{summary}</span>
          <ChevronDownIcon aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        data-loom-plan
        align="start"
        aria-label="Steps"
        className="max-h-(--radix-popover-content-available-height) gap-0 overflow-y-auto p-1.5"
      >
        <ol aria-label="Steps" className="plan-step-list">
          {checklist.steps.map((step) => {
            const current = step.number === currentNumber;
            return (
              <li key={step.number} aria-current={current ? "step" : undefined}>
                <Link
                  to={stepHref(checklist.feature, step)}
                  className="plan-step-list-link"
                  onClick={() => setOpen(false)}
                >
                  <StepGlyph step={step} current={current} live={step.number === liveNumber} />
                  <StepListText step={step} />
                  <span className="sr-only">, {STEP_MARKS[step.mark]}</span>
                </Link>
              </li>
            );
          })}
        </ol>
      </PopoverContent>
    </Popover>
  );
}
