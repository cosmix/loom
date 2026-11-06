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
 * The text that seeds a Plan follow-ups session with one candidate: its ledger id, its title (the
 * first line of its text) and its source, the review file and the stage id. A finding written by
 * hand has no stage id, so the source names the file only.
 */
export function followUpSeed(candidate: CandidateView): string {
  const kind = candidate.kind === "finding" ? "finding" : "question";
  const title = candidate.text.split("\n", 1)[0];
  const stage = candidate.stage_id === "" ? "" : `, stage ${candidate.stage_id}`;
  return `${candidate.id}: ${title} (open ${kind} from ${reviewFile(candidate.review)}${stage})`;
}
