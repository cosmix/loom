import type { PlanModelStats, PlanStageStats, PlanStats } from "@/api/schema";
import { cacheHitPercent, compactTokens, fresh } from "@/lib/tokens";

export const EMPTY = "—";

/// "6h 35m", "45m", "30s": the duration forms of the overview, whole units, no padding.
export function formatSpan(secs: number): string {
  const whole = Math.max(0, Math.floor(secs));
  if (whole < 60) return `${whole}s`;
  const minutes = Math.floor(whole / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

const HOUR_MINUTE = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

/// 24-hour "09:12" in the viewer's zone; the em dash for an unparseable input.
export function clockHm(value: string | number | Date): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? EMPTY : HOUR_MINUTE.format(date);
}

/// Cache hit as "94.9%", or the em dash when nothing was read or written (the header rail's
/// empty display).
export function hitText(record: PlanStats): string {
  const hit = cacheHitPercent(record.totals);
  return hit === null ? EMPTY : `${hit.toFixed(1)}%`;
}

/// Stage execution time over wall time as "1.47× parallel"; null when the wall time is zero.
export function parallelText(record: PlanStats): string | null {
  if (record.wall_secs <= 0) return null;
  return `${(record.execution_secs / record.wall_secs).toFixed(2)}× parallel`;
}

export function freshText(record: PlanStats): string {
  return compactTokens(fresh(record.totals));
}

export function allTokens(record: PlanStats): number {
  const { input, cache_write, cache_read, output } = record.totals;
  return input + cache_write + cache_read + output;
}

export function stageCounts(record: PlanStats): { completed: number; skipped: number } {
  const count = (status: PlanStageStats["status"]) =>
    record.stages.filter((stage) => stage.status === status).length;
  return { completed: count("completed"), skipped: count("skipped") };
}

/// "7 completed · 1 skipped", the skipped part left out when no stage was skipped.
export function countsText(record: PlanStats): string {
  const { completed, skipped } = stageCounts(record);
  return skipped === 0 ? `${completed} completed` : `${completed} completed · ${skipped} skipped`;
}

/// Models by fresh tokens, largest first; ties keep the record's order.
export function modelsByFresh(record: PlanStats): PlanModelStats[] {
  return record.models
    .map((model, index) => ({ model, index, tokens: fresh(model.tokens) }))
    .sort((a, b) => b.tokens - a.tokens || a.index - b.index)
    .map((entry) => entry.model);
}

/// The file the record is archived under in `<loom home>/stats/plans/`: the plan id is already
/// the safe identifier, and Rust's `file_name` appends `.json` to it.
export function archiveFileName(record: PlanStats): string {
  return `${record.plan_id}.json`;
}
