import type { ChecklistView, StepView } from "@/plan/api/schema";

export const STEP_COUNT = 7;

/** Action title of each step; step n uses index n - 1. */
export const STEP_TITLES: readonly string[] = [
  "Write the vision",
  "Write the product spec",
  "Write the metaplan",
  "Write the plans",
  "Pressure-test the plans",
  "Review what was built",
  "Run the plans",
];

/** "1 plan", "2 plans": the count with its noun in the right number. */
export function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function stepTitle(number: number): string {
  return STEP_TITLES[number - 1] ?? `Step ${number}`;
}

/** `?step=<n>` wins when it names one of the seven steps; else the current step, or the last when ready. */
export function selectedStep(raw: string | null, checklist: ChecklistView): number {
  const requested = Number(raw);
  if (raw !== null && String(requested) === raw && requested >= 1 && requested <= STEP_COUNT) {
    return requested;
  }
  return checklist.current.kind === "step" ? checklist.current.number : STEP_COUNT;
}

/** The muted line under a step's label in the navigation. */
export function stepLine(checklist: ChecklistView, step: StepView): string | null {
  const { sections, plans, untested } = checklist;
  switch (step.number) {
    case 1:
      return `${plural(sections.length, "section")} listed`;
    case 2:
      return `${sections.filter((section) => section.written).length} of ${sections.length} written`;
    case 3:
      return step.mark === "done" ? "written" : "after the spec";
    case 4:
      return plural(plans.total, "plan");
    case 5:
      return untested > 0 ? `${untested} untested` : "all tested";
    case 6:
      return "optional";
    case 7:
      return plans.running > 0 ? `${plans.running} running` : `${plans.done} done`;
    default:
      return null;
  }
}
