import type { StepView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";

const MARKS: Record<StepView["mark"], { glyph: string; className: string }> = {
  done: {
    glyph: "✓",
    className: "border border-primary bg-primary text-primary-foreground",
  },
  gap: {
    glyph: "!",
    className: "border border-dashed border-(--tone-warning) text-(--tone-warning)",
  },
  active: {
    glyph: "●",
    className: "border-4 border-double border-primary bg-muted text-primary",
  },
  optional: {
    glyph: "−",
    className: "border border-dotted border-border text-muted-foreground",
  },
};

export function ProgressStrip({ steps }: { steps: StepView[] }) {
  return (
    <ol aria-label="Progress" className="flex gap-1.5">
      {steps.map((step) => (
        <li
          key={step.number}
          className={`flex h-5 min-w-0 flex-1 items-center justify-center rounded-sm text-xs font-semibold ${MARKS[step.mark].className}`}
        >
          <span aria-hidden="true">{MARKS[step.mark].glyph}</span>
          <span className="sr-only">
            <Untrusted text={`${step.name}: ${step.mark}`} />
          </span>
        </li>
      ))}
    </ol>
  );
}
