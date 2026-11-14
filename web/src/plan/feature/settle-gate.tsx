import type { JSX } from "react";

import { useActiveTeam } from "@/plan/feature/use-active-team";
import { SettleAllButton } from "@/plan/trace/settle-all";

/**
 * The bulk settle action, held back while a team still drafts the feature's documents: the specs
 * it would settle are being written, so the count it shows would be wrong by the time it is read.
 */
export function SettleAllAfterDrafting({
  feature,
  role,
}: {
  feature: string;
  role: "section" | "plan";
}): JSX.Element | null {
  const active = useActiveTeam(feature);
  if (active !== null && !active.finished) return null;
  return <SettleAllButton feature={feature} role={role} />;
}
