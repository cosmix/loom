import type { PlanView } from "@/plan/api/schema-files";
import { PlanRunActions } from "@/plan/run/plan-run-actions";

/** Run and follow-up actions at the end of the plan header. */
export function PlanActionsExtras({ plan, path }: { plan: PlanView; path: string }) {
  return <PlanRunActions path={path} state={plan.state} />;
}
