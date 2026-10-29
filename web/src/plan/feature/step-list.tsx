import { CircleCheck, CircleDashed, CircleDot, TriangleAlert } from "lucide-react";

import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { PaceChoice } from "@/plan/feature/pace";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";

const MARKS = {
  done: { Glyph: CircleCheck, label: "done" },
  gap: { Glyph: TriangleAlert, label: "has gaps" },
  active: { Glyph: CircleDot, label: "in progress" },
  optional: { Glyph: CircleDashed, label: "optional" },
};

function StepMark({ mark }: { mark: MarkView }) {
  const { Glyph, label } = MARKS[mark];
  return (
    <span className="inline-flex items-center gap-2 text-sm text-muted-foreground">
      <Glyph className="size-4 shrink-0" aria-hidden="true" />
      <span>{label}</span>
    </span>
  );
}

function StepRow({ checklist, step }: { checklist: ChecklistView; step: StepView }) {
  return (
    <li
      aria-current={isCurrentStep(checklist.current, step) ? "step" : undefined}
      className="border-b border-hairline py-4 last:border-b-0"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-medium text-foreground">
          <Untrusted text={step.name} />
        </h2>
        <StepMark mark={step.mark} />
      </div>
      <PaceChoice checklist={checklist} step={step} />
    </li>
  );
}

export function StepList({ checklist }: { checklist: ChecklistView }) {
  const ready = readyLabel(checklist.current);
  return (
    <div>
      <ol aria-label="Steps" className="list-none p-0">
        <li className="flex flex-wrap items-center justify-between gap-3 border-b border-hairline py-4">
          <h2 className="font-medium text-foreground">Conversation</h2>
          <StepMark mark="done" />
        </li>
        {checklist.steps.map((step) => (
          <StepRow key={step.number} checklist={checklist} step={step} />
        ))}
      </ol>
      {ready && <p className="py-3 text-sm text-muted-foreground">{ready}</p>}
    </div>
  );
}
