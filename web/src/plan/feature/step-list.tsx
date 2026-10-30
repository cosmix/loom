import type { LucideIcon } from "lucide-react";
import { Link } from "react-router";

import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { stepLine } from "@/plan/feature/steps";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { OTHER_GLYPHS, STEP_GLYPHS } from "@/plan/ui/glyphs";

const MARK_LABELS: Record<MarkView, string> = {
  done: "done",
  gap: "has gaps",
  active: "in progress",
  optional: "optional",
};

function featurePath(feature: string): string {
  return `/f/${encodeURIComponent(feature)}`;
}

function Row({
  to,
  mark,
  selected,
  current = false,
  glyph,
  label,
  line,
}: {
  to: string;
  mark: MarkView;
  /** The step the body shows: the only row that carries aria-current. */
  selected: boolean;
  /** The server's current step; marked quietly when the body shows another one. */
  current?: boolean;
  glyph: LucideIcon;
  label: string;
  line: string | null;
}) {
  const Glyph = mark === "done" ? OTHER_GLYPHS.stepDone : glyph;
  return (
    <li>
      <Link
        to={to}
        aria-current={selected ? "step" : undefined}
        data-step-mark={mark}
        data-current={current && !selected ? "" : undefined}
        className="plan-step-row"
      >
        <span className="plan-step-badge">
          <Glyph className="size-3.5" aria-hidden="true" />
        </span>
        <span className="plan-step-text">
          <span className="plan-step-label">
            <Untrusted text={label} />
            <span className="sr-only">, {MARK_LABELS[mark]}</span>
          </span>{" "}
          {line !== null && (
            <span className="plan-step-line">
              <Untrusted text={line} />
            </span>
          )}
        </span>
      </Link>
    </li>
  );
}

function serverRow(checklist: ChecklistView, step: StepView, selected: number) {
  return (
    <Row
      key={step.number}
      to={`${featurePath(checklist.feature)}?step=${step.number}`}
      mark={step.mark}
      selected={step.number === selected}
      current={isCurrentStep(checklist.current, step)}
      glyph={STEP_GLYPHS[step.number - 1] ?? OTHER_GLYPHS.openQuestions}
      label={step.name}
      line={stepLine(checklist, step)}
    />
  );
}

/** The step navigation: Conversation, then one link per server step; `selected` is the step the body shows. */
export function StepList({ checklist, selected }: { checklist: ChecklistView; selected: number }) {
  const ready = readyLabel(checklist.current);
  return (
    <nav aria-label="Steps" className="plan-feature__nav">
      <ol className="plan-step-list">
        <Row
          to={featurePath(checklist.feature)}
          mark="done"
          selected={false}
          glyph={OTHER_GLYPHS.stepDone}
          label="Conversation"
          line={checklist.feature}
        />
        {checklist.steps.map((step) => serverRow(checklist, step, selected))}
      </ol>
      {ready && <p className="plan-step-ready">{ready}</p>}
    </nav>
  );
}
