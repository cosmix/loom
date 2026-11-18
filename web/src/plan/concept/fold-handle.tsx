import { ChevronLeft, ChevronRight } from "lucide-react";
import { useRef, useState, type JSX } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** Room the tooltip keeps from the edge of the reading area before it flips below the cap. */
const COLLISION_PADDING = 8;

const CHEVRON = { start: ChevronLeft, end: ChevronRight } as const;

/**
 * Each end of a run folds it; Escape remains the column's keyboard shortcut. The tooltip sits
 * above the cap and flips below when there is no room above inside the reading area: the
 * planner scrolls its pages inside `main` (plan/shell.tsx), under the app header and the
 * document heading, so `main` is the collision boundary rather than the window. Outside
 * `main` (the help dialog's scenes) the window is.
 */
export function FoldCap({
  runId,
  side,
  onFold,
}: {
  runId: string;
  side: "start" | "end";
  onFold: () => void;
}): JSX.Element {
  const trigger = useRef<HTMLButtonElement>(null);
  const [boundary, setBoundary] = useState<Element | null>(null);
  const Chevron = CHEVRON[side];
  return (
    <Tooltip
      onOpenChange={(open) => {
        if (open) setBoundary(trigger.current?.closest("main") ?? null);
      }}
    >
      <TooltipTrigger asChild>
        <button
          ref={trigger}
          type="button"
          tabIndex={0}
          aria-label="Fold"
          aria-describedby={runId}
          className="concept-run-cap"
          data-side={side}
          onKeyDown={(event) => {
            if (event.key !== "Enter" && event.key !== " ") return;
            event.preventDefault();
            event.stopPropagation();
            onFold();
          }}
          onClick={(event) => {
            event.stopPropagation();
            onFold();
          }}
        >
          <Chevron aria-hidden="true" className="concept-run-cap-icon" strokeWidth={2.5} />
        </button>
      </TooltipTrigger>
      <FoldTooltip boundary={boundary} />
    </Tooltip>
  );
}

function FoldTooltip({ boundary }: { boundary: Element | null }): JSX.Element {
  return (
    <TooltipContent
      side="top"
      avoidCollisions
      collisionBoundary={boundary}
      collisionPadding={COLLISION_PADDING}
    >
      Fold · Esc
    </TooltipContent>
  );
}
