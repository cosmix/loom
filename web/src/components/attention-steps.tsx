import type { ReactNode } from "react";

import type { Attention } from "@/api/schema";
import { CopyCommand } from "@/components/copy-command";

/// The three decisions `loom stage human-review` accepts (`human_review.rs`).
const REVIEW_CHOICES = ["--approve", "--force-complete", '--reject "<reason>"'] as const;

interface Step {
  lead: string | null;
  command: string;
}

/// The entry's command, introduced by when to run it, then each alternative.
function stepsOf(entry: Attention): Step[] {
  const steps: Step[] = [];
  if (entry.command) {
    steps.push({
      lead: entry.command_when ? `Once ${entry.command_when}:` : null,
      command: entry.command,
    });
  }
  for (const alternative of entry.alternatives) {
    steps.push({ lead: `If ${alternative.when}:`, command: alternative.command });
  }
  return steps;
}

/// Whether the entry offers anything to run.
export function hasSteps(entry: Attention): boolean {
  return entry.command !== null || entry.alternatives.length > 0 || entry.has_human_review_choices;
}

/// The commands an attention entry offers, numbered in the order the entry
/// gives them: the command, each alternative, then the review decisions.
/// Every command keeps its own copy button; the sentences around them are
/// prose and never get one.
export function AttentionSteps({ entry }: { entry: Attention }) {
  if (!hasSteps(entry)) return null;
  const steps = stepsOf(entry);
  return (
    <ol className="attention-steps" aria-label="steps">
      {steps.map((step, index) => (
        // Two situations may share a command, so its place keys it too.
        <StepItem key={`${index}:${step.command}`} number={index + 1}>
          {step.lead && <p className="text-xs wrap-anywhere text-muted-foreground">{step.lead}</p>}
          <CopyCommand command={step.command} />
        </StepItem>
      ))}
      {entry.has_human_review_choices && (
        <StepItem number={steps.length + 1}>
          <ReviewChoices id={entry.id} />
        </StepItem>
      )}
    </ol>
  );
}

function StepItem({ number, children }: { number: number; children: ReactNode }) {
  return (
    <li className="attention-step">
      <span className="index-badge">{number}</span>
      <div className="flex min-w-0 flex-col gap-1.5">{children}</div>
    </li>
  );
}

function ReviewChoices({ id }: { id: string }) {
  return (
    <ul className="flex flex-wrap gap-1.5" aria-label="review decisions">
      {REVIEW_CHOICES.map((choice) => (
        <li key={choice} className="max-w-full min-w-0">
          <CopyCommand command={`loom stage human-review ${id} ${choice}`} />
        </li>
      ))}
    </ul>
  );
}
