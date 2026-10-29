import { ChevronLeft, ChevronRight } from "lucide-react";
import type { JSX } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** Each end of a run folds it; Escape remains the column's keyboard shortcut. */
export function FoldCap({
  runId,
  side,
  onFold,
}: {
  runId: string;
  side: "start" | "end";
  onFold: () => void;
}): JSX.Element {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
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
          {side === "start" ? (
            <ChevronLeft aria-hidden="true" className="concept-run-cap-icon" strokeWidth={2.5} />
          ) : (
            <ChevronRight aria-hidden="true" className="concept-run-cap-icon" strokeWidth={2.5} />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>Fold · Esc</TooltipContent>
    </Tooltip>
  );
}
