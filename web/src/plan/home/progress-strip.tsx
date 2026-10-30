import type { StepView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";

const RUN_STEP = 6;

type Cell = "done" | "current" | "running" | "idle";

function cellOf(step: StepView, current: number | null, running: boolean): Cell {
  if (step.number === RUN_STEP && running) return "running";
  if (step.mark === "done") return "done";
  return step.number === current ? "current" : "idle";
}

/**
 * One item per step: done, the current step, a running Run step, or the rest. The list is for
 * screen readers; the card draws the counts as text (DESIGN.md allows no segmented progress bar).
 */
export function ProgressStrip({
  steps,
  current = null,
  running = false,
}: {
  steps: StepView[];
  current?: number | null;
  running?: boolean;
}) {
  return (
    <ol aria-label="Progress" className="sr-only">
      {steps.map((step) => {
        const cell = cellOf(step, current, running);
        return (
          <li key={step.number} data-cell={cell}>
            <Untrusted text={`${step.name}: ${cell === "running" ? "running" : step.mark}`} />
          </li>
        );
      })}
    </ol>
  );
}
