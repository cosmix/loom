import type { PlanView } from "@/plan/api/schema-files";

/// Extension slot at the end of the plan header. It renders nothing in this
/// plan; plan 11 adds "Pressure-test" for written plans and the done-plan
/// actions here, which is why it already receives the plan and its path.
export function PlanActionsExtras(_props: { plan: PlanView; path: string }): null {
  return null;
}
