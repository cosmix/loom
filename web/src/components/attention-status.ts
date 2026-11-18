import type { Attention } from "@/api/schema";
import { isCompletionState, type DisplayState } from "@/lib/display-state";
import { formatElapsed, hazardTone } from "@/lib/format";
import { failureLabel } from "@/lib/format-activity";

/// The state behind an attention entry: the tone and glyph every surface
/// showing the entry wears, the attention card, the now card and the
/// history's current row alike. `shown` is the stage's own `displayState`:
/// loom files a completion blocker's entry under the generic blocked status,
/// so an executing stage held by one wears the blocker, as its card does.
export function attentionStatus(entry: Attention, shown: DisplayState | null): DisplayState {
  return isCompletionState(shown) ? shown : entry.status;
}

/// A state loom is resolving itself is never an error for the operator.
export function attentionHazard(entry: Attention): "error" | "warning" {
  if (entry.automatic) return "warning";
  return hazardTone(entry.status) ?? "error";
}

/// The short phrase the hazard strip shows beside the label: the failure's
/// label, then the adjudication numbers (dispute count, judge heartbeat age).
/// Never the reason: the body below shows that whole, once.
export function attentionDetail(entry: Attention): string | null {
  const parts: string[] = [];
  if (entry.failure_type) parts.push(entry.failure_label ?? failureLabel(entry.failure_type));
  if (entry.dispute_count !== null) parts.push(`${entry.dispute_count} disputed`);
  if (entry.judge_heartbeat_secs !== null) {
    parts.push(`judge heard ${formatElapsed(entry.judge_heartbeat_secs)} ago`);
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}
