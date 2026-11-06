import type { CandidateView, ChecklistView } from "@/plan/api/schema";
import { allPlansDone, livePlans } from "@/plan/feature/run-scope";
import { plural } from "@/plan/ui/plural";

/** The offer id of Plan follow-ups: what a follow-up start sends as its offer. */
export const PLAN_FOLLOW_UPS_ID = "plan-follow-ups";

export interface FollowUpFeature {
  name: string;
  /** Every plan is done (`isBuilt`). */
  built: boolean;
  /** Live plan rows in state done. */
  done: number;
  /** Live plan rows. */
  total: number;
  status: string;
  /** The open entries of its findings ledger. */
  candidates: CandidateView[];
}

/**
 * Every plan is done: the server says the feature is ready and all done, and every live plan row
 * is done (a row still written or running is not built, and a feature with no live plan is not).
 */
function isBuilt(checklist: ChecklistView): boolean {
  return checklist.current.kind === "ready_all_done" && allPlansDone(checklist);
}

/** "done · 13 plans", "in progress · 4 of 6 plans", or "no plans yet". */
export function featureStatus(built: boolean, done: number, total: number): string {
  if (total === 0) return "no plans yet";
  return built
    ? `done · ${plural(total, "plan")}`
    : `in progress · ${done} of ${plural(total, "plan")}`;
}

/**
 * The line a candidate puts in the follow-up field: its ledger id, its title (the first line of
 * its text) and its source, the review file and the stage id. A finding written by hand has no
 * stage id, so the source names the file only.
 */
export function followUpSeed(candidate: CandidateView): string {
  const kind = candidate.kind === "finding" ? "finding" : "question";
  const title = candidate.text.split("\n", 1)[0];
  const file = candidate.review.slice(candidate.review.lastIndexOf("/") + 1);
  const stage = candidate.stage_id === "" ? "" : `, stage ${candidate.stage_id}`;
  return `${candidate.id}: ${title} (open ${kind} from ${file}${stage})`;
}

function followUpFeature(checklist: ChecklistView): FollowUpFeature {
  const live = livePlans(checklist);
  const done = live.filter((row) => row.state === "done").length;
  const built = isBuilt(checklist);
  return {
    name: checklist.feature,
    built,
    done,
    total: live.length,
    status: featureStatus(built, done, live.length),
    candidates: checklist.candidates ?? [],
  };
}

/** The features a follow-up can be for: every feature with a metaplan, done or not, in order. */
export function followUpFeatures(checklists: ChecklistView[]): FollowUpFeature[] {
  return checklists.filter((checklist) => checklist.metaplans.length > 0).map(followUpFeature);
}
