import type { Busy, DaemonState, StageStatus, StageSummary } from "@/api/schema";
import { displayState, type DisplayState } from "@/lib/display-state";

export type Tone =
  | "executing"
  | "completed"
  | "blocked"
  | "pending"
  | "queued"
  | "warning"
  | "merged"
  | "dimmed"
  | "neutral"
  | "contract";

export interface StateMeta {
  tone: Tone;
  bold: boolean;
}

const STATE_META: Record<DisplayState, StateMeta> = {
  "waiting-for-deps": { tone: "pending", bold: false },
  queued: { tone: "queued", bold: true },
  executing: { tone: "executing", bold: true },
  "waiting-for-input": { tone: "warning", bold: true },
  blocked: { tone: "blocked", bold: true },
  completed: { tone: "completed", bold: true },
  "needs-handoff": { tone: "warning", bold: true },
  skipped: { tone: "dimmed", bold: false },
  "merge-conflict": { tone: "warning", bold: true },
  "completed-with-failures": { tone: "blocked", bold: true },
  "merge-blocked": { tone: "blocked", bold: true },
  "needs-human-review": { tone: "warning", bold: false },
  "needs-adjudication": { tone: "warning", bold: true },
  "completion-pending": { tone: "warning", bold: true },
  "completion-blocked": { tone: "blocked", bold: true },
};

export function formatElapsed(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  }
  if (seconds < 3600) {
    return `${Math.trunc(seconds / 60)}m${seconds % 60}s`;
  }
  return `${Math.trunc(seconds / 3600)}h${Math.trunc((seconds % 3600) / 60)}m`;
}

export function stateMeta(status: DisplayState): StateMeta {
  return STATE_META[status];
}

/// Which caution tape a state wears: red for a failure the daemon could not
/// get past, amber for a state waiting on a person or a merge, none otherwise.
export function hazardTone(status: DisplayState): "error" | "warning" | null {
  switch (status) {
    case "blocked":
    case "completed-with-failures":
    case "merge-blocked":
    case "completion-blocked":
      return "error";
    case "merge-conflict":
    case "needs-human-review":
    case "needs-adjudication":
    case "waiting-for-input":
    case "needs-handoff":
    case "completion-pending":
      return "warning";
    default:
      return null;
  }
}

export function contextUsage(stage: StageSummary): {
  tokens: number;
  ceiling: number;
  percent: number;
  /// Segments of the five-cell meter to light: a cell lights once usage
  /// passes its midpoint, so 39% shows two cells, not one.
  filled: number;
  health: "green" | "yellow" | "red";
} | null {
  const applicable = ["executing", "waiting-for-input", "needs-handoff"].includes(stage.status);
  const tokens = stage.context_tokens;
  const ceiling = stage.context_ceiling_tokens;
  if (!applicable || tokens === null || ceiling === null || ceiling === 0) {
    return null;
  }
  const ratio = tokens / ceiling;
  return {
    tokens,
    ceiling,
    percent: Math.round(ratio * 100),
    filled: Math.max(0, Math.min(5, Math.round(ratio * 5))),
    health: ratio >= 0.9 ? "red" : ratio >= 0.6 ? "yellow" : "green",
  };
}

export function modelsOf(stage: StageSummary): { model: string; execution: string[] } {
  return { model: stage.model, execution: [...stage.execution_models] };
}

export function timeText(stage: StageSummary): string | null {
  if (stage.status === "waiting-for-deps" || stage.status === "queued") {
    return null;
  }
  // Completion covers the whole run, including contract writing and phase transitions.
  const completed = stage.status === "completed" || stage.status === "completed-with-failures";
  const seconds = completed
    ? (stage.elapsed_secs ?? stage.execution_secs)
    : (stage.execution_secs ?? stage.elapsed_secs);
  return seconds === null ? "" : formatElapsed(seconds);
}

export function mergeText(stage: StageSummary): { text: string; tone: Tone } | null {
  if (stage.stash_warning) {
    return { text: "stash!", tone: "blocked" };
  }
  if (
    stage.status === "completed" &&
    stage.cleanup_warning !== null &&
    stage.cleanup_warning !== undefined
  ) {
    return { text: "cleanup!", tone: "warning" };
  }
  if (stage.status === "completed" && stage.stage_type !== "knowledge") {
    if (!stage.merged) {
      return { text: "unmerged", tone: "warning" };
    }
    return stage.merge_assumed
      ? { text: "assumed", tone: "merged" }
      : { text: "merged", tone: "merged" };
  }
  if (stage.status === "merge-conflict") {
    return { text: "conflict", tone: "warning" };
  }
  if (stage.status === "merge-blocked") {
    return { text: stage.merge_block ? "blocked" : "error", tone: "blocked" };
  }
  return null;
}

export function progressPercent(completed: number, total: number): number {
  if (total === 0) {
    return 0;
  }
  return Math.trunc((completed * 100 + Math.trunc(total / 2)) / total);
}

// An unparseable `started_at` (NaN) counts as just started.
function busyElapsedSecs(busy: Busy, nowMs: number): number {
  // Truncates like the daemon's `num_seconds`, so both sides agree on a ceiling boundary.
  const elapsed = Math.floor((nowMs - Date.parse(busy.started_at)) / 1000);
  return Number.isNaN(elapsed) ? 0 : Math.max(0, elapsed);
}

/// `staleSecs` is null while the page has a live feed; a number is how long
/// the page has been without one, in which case the daemon's real state is
/// unknown and must not be reported as running.
///
/// `busy` is the operation the scheduler thread is wrapped in. It only shows
/// for a daemon whose tick is fresh: a stale tick wins (and prints the tick's
/// age), and a stopped daemon never shows a leftover busy line. Elapsed time
/// comes from `nowMs` against the daemon's `started_at`.
export function daemonLine(
  daemon: DaemonState,
  tickAgeSecs: number | null,
  staleSecs: number | null,
  busy: Busy | null,
  nowMs: number,
): { text: string; detail?: string; tone: Tone } {
  if (staleSecs !== null) {
    return {
      text: "daemon unknown",
      detail: `no data for ${formatElapsed(staleSecs)}`,
      tone: "dimmed",
    };
  }
  if (daemon === "process-only") {
    return { text: "daemon process alive, socket missing", tone: "warning" };
  }
  if (daemon === "not-running") {
    return { text: "daemon stopped", tone: "dimmed" };
  }
  // The stall check comes before `unreachable`: the tick file is read from
  // disk either way, so a stalled loop is a fact this caller can still see,
  // and it is the more urgent one.
  if (tickAgeSecs !== null && tickAgeSecs >= 60) {
    return { text: `loop stalled ${formatElapsed(tickAgeSecs)}`, tone: "warning" };
  }
  const elapsed = busy ? busyElapsedSecs(busy, nowMs) : 0;
  if (busy && elapsed >= busy.global_ceiling_secs) {
    return { text: `loop stalled ${formatElapsed(elapsed)}`, tone: "warning" };
  }
  if (daemon === "unreachable") {
    return { text: "daemon running", detail: "socket unreachable", tone: "warning" };
  }
  if (busy) {
    return {
      text: "daemon running",
      detail: `busy ${busy.what} ${formatElapsed(elapsed)}`,
      tone: elapsed >= busy.ceiling_secs ? "warning" : "completed",
    };
  }
  return {
    text: "daemon running",
    detail: tickAgeSecs === null ? "tick unknown" : `tick ${tickAgeSecs}s ago`,
    tone: "completed",
  };
}

const CLOCK_FORMAT = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hourCycle: "h23",
});

const STAMP_FORMAT = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hourCycle: "h23",
});

/// 24-hour "14:03:22"; "—" for an unparseable input.
export function formatClock(value: string | number | Date): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "—" : CLOCK_FORMAT.format(date);
}

const DAY_FORMAT = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

const YEAR_DAY_FORMAT = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
});

/// The calendar day of `value` when it is not `now`'s: "Oct 27", with the
/// year when that differs too. Null for today and for an unparseable input,
/// so a time column shows the clock alone for today's events.
export function formatDayUnlessToday(
  value: string | number | Date,
  now: Date = new Date(),
): string | null {
  const date = new Date(value);
  if (Number.isNaN(date.getTime()) || date.toDateString() === now.toDateString()) return null;
  return (date.getFullYear() === now.getFullYear() ? DAY_FORMAT : YEAR_DAY_FORMAT).format(date);
}

/// 24-hour date + time; "—" for an unparseable input.
export function formatStamp(value: string | number | Date): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "—" : STAMP_FORMAT.format(date);
}

export function summaryCounts(
  stages: readonly StageSummary[],
  attentionCount: number,
): { executing: number; queued: number; waiting: number; attention: number; done: number } {
  // An executing stage held by a completion blocker counts under attention.
  const count = (status: StageStatus) =>
    stages.filter((stage) => displayState(stage) === status).length;
  return {
    executing: count("executing"),
    queued: count("queued"),
    waiting: count("waiting-for-deps"),
    attention: attentionCount,
    done: count("completed"),
  };
}
