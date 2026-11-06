import type { StepView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { STEP } from "@/plan/feature/steps";
import { LiveDot } from "@/plan/ui/activity-glyphs";
import { STEP_MARK_GLYPHS } from "@/plan/ui/glyphs";

type Cell = "done" | "current" | "running" | "idle";

function cellOf(step: StepView, current: number | null, running: boolean): Cell {
  if (step.number === STEP.run && running) return "running";
  if (step.mark === "done") return "done";
  return step.number === current ? "current" : "idle";
}

/**
 * The six-cell strip: one glyph per step in workflow order, the spine's step marks (a check for
 * done, a dashed circle for a gap), the current step in --foreground and a running Run step as
 * the live dot. Each glyph's word is in the cell for assistive technology and is the cell's
 * tooltip, as the spine's plan badges are (ui/DESIGN.md, Surfaces and borders).
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
    <ol aria-label="Progress" className="progress-strip">
      {steps.map((step) => {
        const cell = cellOf(step, current, running);
        const Glyph = cell === "running" ? LiveDot : STEP_MARK_GLYPHS[step.mark];
        const text = `${step.name}: ${cell === "running" ? "running" : step.mark}`;
        return (
          <li key={step.number} data-cell={cell} className="progress-strip__cell">
            <span
              aria-hidden="true"
              title={visibleControls(text)}
              className="progress-strip__glyph"
            >
              <Glyph aria-hidden="true" />
            </span>
            <span className="sr-only">
              <Untrusted text={text} />
            </span>
          </li>
        );
      })}
    </ol>
  );
}
