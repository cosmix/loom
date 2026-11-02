import type { Attention, StageStatus } from "@/api/schema";
import { formatElapsed, hazardTone } from "@/lib/format";
import { failureLabel } from "@/lib/format-activity";

/// The state behind an attention entry: the tone and glyph every surface
/// showing the entry wears, the attention card, the now card and the
/// history's current row alike.
export function attentionStatus(entry: Attention): StageStatus {
  return entry.status;
}

/// A state loom is resolving itself is never an error for the operator.
export function attentionHazard(entry: Attention): "error" | "warning" {
  if (entry.automatic) return "warning";
  return hazardTone(attentionStatus(entry)) ?? "error";
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
