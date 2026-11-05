import type { BuiltView, CandidateView, ChecklistView } from "@/plan/api/schema";
import { livePlans } from "@/plan/feature/run-scope";
import { sourceHref } from "@/plan/feature/source-href";

export const EMPTY_BUILT: BuiltView = { reviews: [], spec: [], candidates: [] };

/** The checklist's built view; the server always sends one, hand-built checklists may omit it. */
export function builtOf(checklist: ChecklistView): BuiltView {
  return checklist.built ?? EMPTY_BUILT;
}

/**
 * Every plan is done: the server says the feature is ready and all done, and every live plan row
 * is done (a row still written or running is not built, and a feature with no live plan is not).
 */
export function isBuilt(checklist: ChecklistView): boolean {
  const live = livePlans(checklist);
  return (
    checklist.current.kind === "ready_all_done" &&
    live.length > 0 &&
    live.every((row) => row.state === "done")
  );
}

/** The last segment of a repository path: a run review's file name. */
export function reviewFile(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** Where a run review opens: inside its feature when it lives there, else the file browser. */
export function reviewHref(path: string): string {
  return sourceHref(path);
}

/**
 * The text that seeds a Plan follow-ups session with one candidate and its source. The source is
 * the review file and the stage id; the stage name stays on screen only.
 */
export function followUpSeed(candidate: CandidateView): string {
  const kind = candidate.kind === "finding" ? "finding" : "question";
  return `${candidate.text} (open ${kind} from ${reviewFile(candidate.review)}, stage ${candidate.stage_id})`;
}
