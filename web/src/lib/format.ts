import type { DaemonState, FailureType, StageStatus, StageSummary } from "@/api/schema";

export type Tone =
  | "executing"
  | "completed"
  | "blocked"
  | "pending"
  | "queued"
  | "warning"
  | "merged"
  | "dimmed"
  | "neutral";

export interface StateMeta {
  icon: string;
  label: string;
  tone: Tone;
  bold: boolean;
}

const STATE_META: Record<StageStatus, StateMeta> = {
  "waiting-for-deps": { icon: "○", label: "Waiting", tone: "pending", bold: false },
  queued: { icon: "▶", label: "Queued", tone: "queued", bold: true },
  executing: { icon: "●", label: "Executing", tone: "executing", bold: true },
  "waiting-for-input": { icon: "?", label: "Input", tone: "warning", bold: true },
  blocked: { icon: "✗", label: "Blocked", tone: "blocked", bold: true },
  completed: { icon: "✓", label: "Completed", tone: "completed", bold: true },
  "needs-handoff": { icon: "⟳", label: "Handoff", tone: "warning", bold: true },
  skipped: { icon: "⊘", label: "Skipped", tone: "dimmed", bold: false },
  "merge-conflict": { icon: "⚡", label: "Conflict", tone: "warning", bold: true },
  "completed-with-failures": { icon: "⚠", label: "Failed", tone: "blocked", bold: true },
  "merge-blocked": { icon: "⊗", label: "MergeBlk", tone: "blocked", bold: true },
  "needs-human-review": { icon: "⏸", label: "Review", tone: "warning", bold: false },
  "needs-adjudication": { icon: "⚖", label: "Adjudicate", tone: "warning", bold: true },
};

export const LEGEND: ReadonlyArray<{ status: StageStatus; meaning: string }> = [
  { status: "waiting-for-deps", meaning: "waiting for its dependencies to complete and merge" },
  { status: "queued", meaning: "ready; the daemon spawns a session when a slot frees up" },
  { status: "executing", meaning: "a session is working in the stage's worktree" },
  { status: "waiting-for-input", meaning: "the agent is waiting for an answer in its session" },
  { status: "needs-handoff", meaning: "context ceiling reached; a fresh session resumes it" },
  { status: "completed", meaning: "work done and verified — may still be unmerged" },
  { status: "skipped", meaning: "skipped by you; does not satisfy its dependents" },
  { status: "blocked", meaning: "errored; needs intervention → loom stage retry <id>" },
  {
    status: "completed-with-failures",
    meaning: "acceptance failed; retried automatically up to the limit",
  },
  { status: "merge-conflict", meaning: "merge conflict → loom stage merge <id>" },
  { status: "merge-blocked", meaning: "merge errored (not a conflict) → loom stage merge <id>" },
  { status: "needs-human-review", meaning: "asked for a decision → loom stage human-review <id>" },
  {
    status: "needs-adjudication",
    meaning: "a disputed acceptance criterion awaits the judge's verdict",
  },
];

export function formatElapsed(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  }
  if (seconds < 3600) {
    return `${Math.trunc(seconds / 60)}m${seconds % 60}s`;
  }
  return `${Math.trunc(seconds / 3600)}h${Math.trunc((seconds % 3600) / 60)}m`;
}

export function stateMeta(status: StageStatus): StateMeta {
  return STATE_META[status];
}

export function failureLabel(type: FailureType): string {
  const labels: Record<FailureType, string> = {
    "session-crash": "crash",
    "test-failure": "test",
    "build-failure": "build",
    "code-error": "code",
    timeout: "timeout",
    "context-exhausted": "context",
    "user-blocked": "user",
    "merge-conflict": "merge",
    "infrastructure-error": "infra",
    "sandbox-setup-failure": "sandbox",
    "startup-refusal": "startup",
    unknown: "error",
  };
  return labels[type];
}

function retryText(stage: StageSummary, label: string): { text: string; tone: Tone } {
  return {
    text: `${label} ${stage.retry_count}/${stage.max_retries ?? 3}`,
    tone: "blocked",
  };
}

function executingActivity(stage: StageSummary): { text: string; tone: Tone } {
  switch (stage.activity_status) {
    case "Working":
      return {
        text: stage.last_tool ? `working · ${stage.last_tool}` : "working",
        tone: "completed",
      };
    case "Idle":
      return stalenessText("idle", stage.staleness_secs, "dimmed");
    case "Stale":
      return stalenessText("stale", stage.staleness_secs, "warning");
    case "Orphaned":
      return { text: "orphaned", tone: "blocked" };
    case "Error":
      return { text: "crashed", tone: "blocked" };
  }
}

function stalenessText(
  prefix: string,
  seconds: number | null,
  tone: Tone,
): { text: string; tone: Tone } {
  return { text: seconds === null ? prefix : `${prefix} ${formatElapsed(seconds)}`, tone };
}

function baseActivity(stage: StageSummary): { text: string; tone: Tone } | null {
  if (stage.status === "executing" && stage.incoherence !== null) {
    return { text: "incoherent", tone: "blocked" };
  }
  switch (stage.status) {
    case "queued":
      return { text: "ready", tone: "queued" };
    case "executing":
      return executingActivity(stage);
    case "waiting-for-input":
      return { text: "awaiting input", tone: "warning" };
    case "needs-handoff":
      return { text: "handing off", tone: "warning" };
    case "blocked":
      return retryText(
        stage,
        stage.failure_info ? failureLabel(stage.failure_info.failure_type) : "error",
      );
    case "completed-with-failures":
      return retryText(stage, "failed");
    case "merge-conflict":
      return { text: "conflict", tone: "warning" };
    case "merge-blocked":
      return { text: "merge error", tone: "blocked" };
    case "needs-human-review":
      return { text: "awaiting you", tone: "warning" };
    case "needs-adjudication":
      return adjudicationText(stage);
    default:
      return null;
  }
}

function adjudicationText(stage: StageSummary): { text: string; tone: Tone } {
  const judge =
    stage.judge_heartbeat_secs === null
      ? "none"
      : stage.judge_heartbeat_secs <= 300
        ? "working"
        : "stale";
  return { text: `dispute ${stage.dispute_count} · judge ${judge}`, tone: "warning" };
}

export function activityText(stage: StageSummary): { text: string; tone: Tone } | null {
  const activity = baseActivity(stage);
  if (!activity || !stage.held) {
    return activity;
  }
  return { text: `held · ${activity.text}`, tone: "warning" };
}

export function contextUsage(stage: StageSummary): {
  tokens: number;
  ceiling: number;
  percent: number;
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
    filled: Math.max(0, Math.min(5, Math.floor(ratio * 5))),
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
  const seconds = stage.execution_secs ?? stage.elapsed_secs;
  return seconds === null ? "" : formatElapsed(seconds);
}

export function mergeText(stage: StageSummary): { text: string; tone: Tone } | null {
  if (
    stage.status === "completed" &&
    stage.cleanup_warning !== null &&
    stage.cleanup_warning !== undefined
  ) {
    return { text: "cleanup!", tone: "warning" };
  }
  if (stage.status === "completed" && stage.stage_type !== "knowledge") {
    return stage.merged
      ? { text: "merged", tone: "merged" }
      : { text: "unmerged", tone: "warning" };
  }
  if (stage.status === "merge-conflict") {
    return { text: "conflict", tone: "warning" };
  }
  if (stage.status === "merge-blocked") {
    return { text: "error", tone: "blocked" };
  }
  return null;
}

export function progressPercent(completed: number, total: number): number {
  if (total === 0) {
    return 0;
  }
  return Math.trunc((completed * 100 + Math.trunc(total / 2)) / total);
}

export function daemonLine(
  daemon: DaemonState,
  tickAgeSecs: number | null,
): { text: string; detail?: string; tone: Tone } {
  if (daemon === "process-only") {
    return { text: "daemon process alive, socket missing", tone: "warning" };
  }
  if (daemon === "not-running") {
    return { text: "daemon stopped", tone: "dimmed" };
  }
  if (tickAgeSecs !== null && tickAgeSecs >= 60) {
    return { text: `loop stalled ${tickAgeSecs}s`, tone: "warning" };
  }
  return {
    text: "daemon running",
    detail: tickAgeSecs === null ? "tick unknown" : `tick ${tickAgeSecs}s ago`,
    tone: "completed",
  };
}

export function summaryCounts(
  stages: readonly StageSummary[],
  attentionCount: number,
): { executing: number; queued: number; waiting: number; attention: number; done: number } {
  const count = (status: StageStatus) => stages.filter((stage) => stage.status === status).length;
  return {
    executing: count("executing"),
    queued: count("queued"),
    waiting: count("waiting-for-deps"),
    attention: attentionCount,
    done: count("completed"),
  };
}
