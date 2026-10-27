import {
  CheckIcon,
  ChevronDownIcon,
  CircleAlertIcon,
  CircleDashedIcon,
  CircleDotIcon,
  type LucideIcon,
} from "lucide-react";

import { cn } from "cn";
import { Button } from "@/components/ui/button";
import type { ChecklistView, CurrentView, MarkView, StepView } from "@/plan/api/schema";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";

const MARKS: Record<MarkView, { icon: LucideIcon; label: string; tone: string }> = {
  done: { icon: CheckIcon, label: "done", tone: "text-(--tone-completed)" },
  gap: { icon: CircleAlertIcon, label: "has gaps", tone: "text-(--tone-warning)" },
  active: { icon: CircleDotIcon, label: "in progress", tone: "text-(--tone-executing)" },
  optional: { icon: CircleDashedIcon, label: "optional", tone: "text-(--tone-dimmed)" },
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

/** The step list; the mark is a glyph plus a label for screen readers. */
export function StepList({
  checklist,
  className,
  showNames = true,
}: {
  checklist: ChecklistView;
  className?: string;
  showNames?: boolean;
}) {
  return (
    <ol aria-label="Steps" className={className}>
      {checklist.steps.map((step) => {
        const current = isCurrentStep(checklist.current, step);
        const mark = MARKS[step.mark];
        return (
          <li
            key={step.number}
            aria-current={current ? "step" : undefined}
            className={cn(
              "flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-xs font-medium",
              current ? "bg-muted text-foreground" : "text-muted-foreground",
            )}
          >
            <mark.icon className={cn("size-3.5 shrink-0", mark.tone)} aria-hidden="true" />
            <span>{step.number}</span>
            <span className={cn(!showNames && !current && "sr-only xl:not-sr-only")}>
              {step.name}
            </span>
            <span className="sr-only">, {mark.label}</span>
          </li>
        );
      })}
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
          <Button variant="ghost" size="sm" className="min-w-0 shrink text-xs">
            <span className="truncate">{summary(checklist)}</span>
            <ChevronDownIcon aria-hidden="true" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" aria-label="Steps">
          <StepList checklist={checklist} className="flex flex-col gap-0.5" />
        </PopoverContent>
      </Popover>
    );
  }
  const ready = readyLabel(checklist.current);
  return (
    <div className="flex min-w-0 items-center gap-2">
      <StepList checklist={checklist} className="plan-steps" showNames={false} />
      {ready && <span className="text-xs font-medium text-(--tone-completed)">{ready}</span>}
    </div>
  );
}
