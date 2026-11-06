import type { ChecklistView } from "@/plan/api/schema";

/** What a feature card says needs the reader, read from the checklist the server sends. */
export interface Attention {
  /** Plans of the feature `loom run` is executing now. */
  running: number;
  /** Open questions across the product spec's sections. */
  questions: number;
  /** Findings and questions the last run review left open. */
  findings: number;
  /** Written plans with no pressure test on record. */
  untested: number;
  /** Every step of the checklist is done. */
  allDone: boolean;
  /** Every step is done and nothing above asks for a look. */
  quiet: boolean;
}

export function attentionOf(checklist: ChecklistView): Attention {
  const running = checklist.plans.running;
  const questions = checklist.sections.reduce((sum, section) => sum + section.open_questions, 0);
  const findings = checklist.candidates?.length ?? 0;
  const allDone =
    checklist.steps.length > 0 && checklist.steps.every((step) => step.mark === "done");
  return {
    running,
    questions,
    findings,
    untested: checklist.untested,
    allDone,
    quiet: allDone && running + questions + findings === 0,
  };
}

/**
 * Where a feature sorts on Home: work running now, then a feature with questions or findings to
 * answer, then work under way, then finished work. Lower comes first.
 */
export function attentionRank(attention: Attention): number {
  if (attention.running > 0) return 0;
  if (attention.questions + attention.findings > 0) return 1;
  return attention.quiet ? 3 : 2;
}

/** The features in reading order: by rank, and the server's order inside a rank. */
export function byAttention(checklists: readonly ChecklistView[]): ChecklistView[] {
  return checklists
    .map((checklist, index) => ({ checklist, index, rank: attentionRank(attentionOf(checklist)) }))
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map(({ checklist }) => checklist);
}
