import type { PlanStats } from "@/api/schema";
import { compactTokens } from "@/lib/tokens";

import { allTokens, countsText, formatSpan, freshText, hitText } from "./plan-math";

/// The one-line summary the Copy summary button puts on the clipboard.
export function summaryText(record: PlanStats): string {
  return [
    `${record.plan_name}: ${countsText(record)}`,
    `${formatSpan(record.wall_secs)} wall`,
    `${formatSpan(record.execution_secs)} execution`,
    `${freshText(record)} fresh tokens`,
    `${hitText(record)} cache hit`,
    `${compactTokens(allTokens(record))} tokens total`,
  ].join(", ");
}
