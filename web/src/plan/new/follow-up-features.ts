import type { CandidateView, ChecklistView } from "@/plan/api/schema";
import { builtOf, isBuilt } from "@/plan/feature/built-model";
import { livePlans } from "@/plan/feature/run-scope";
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
  /** The open findings and questions of its run reviews. */
  candidates: CandidateView[];
}

/** "done · 13 plans", "in progress · 4 of 6 plans", or "no plans yet". */
export function featureStatus(built: boolean, done: number, total: number): string {
  if (total === 0) return "no plans yet";
  return built
    ? `done · ${plural(total, "plan")}`
    : `in progress · ${done} of ${plural(total, "plan")}`;
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
    candidates: builtOf(checklist).candidates,
  };
}

/** The features a follow-up can be for: every feature with a metaplan, done or not, in order. */
export function followUpFeatures(checklists: ChecklistView[]): FollowUpFeature[] {
  return checklists.filter((checklist) => checklist.metaplans.length > 0).map(followUpFeature);
}
