import type { FailureType, OutgoingSessionExitReason, StageSummary } from "@/api/schema";
import { formatElapsed, type Tone } from "@/lib/format";

/// A v2 standard stage's contract-writer phase: a "stage" session status but
/// spawned to write and freeze failing contract tests before the
/// implementing session starts. The wire status stays "executing" through
/// this phase; only `session_type` tells the two apart.
export function isContractPhase(stage: StageSummary): boolean {
  return stage.status === "executing" && stage.session_type === "contract";
}

/// `session_type` as the raw wire value except "contract", which reads as
/// what it spawns rather than the phase it belongs to.
export function sessionTypeLabel(type: StageSummary["session_type"]): string | null {
  if (type === null) return null;
  return type === "contract" ? "contract writer" : type;
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

/// A session with a tool call open is working even though its heartbeat is
/// silent, so the activity instruments treat it like `Working`.
export function sessionIsWorking(stage: StageSummary): boolean {
  return stage.activity_status === "Working" || stage.activity_status === "Busy";
}

/// The sentence for a contract writer blocked on its frozen contracts' host
/// job, `contracts written, testing 9/11 · 6m12s`; null when none applies. An
/// orphaned or crashed writer is excluded so its silence never reads as healthy.
export function contractTesting(stage: StageSummary): string | null {
  const testing = stage.contract_testing;
  if (!testing || !isContractPhase(stage)) return null;
  if (stage.activity_status === "Orphaned" || stage.activity_status === "Error") return null;
  if (testing.queued) return "contracts written, waiting to test";
  const elapsed = testing.running_secs === null ? "" : ` · ${formatElapsed(testing.running_secs)}`;
  return `contracts written, testing ${testing.done}/${testing.total}${elapsed}`;
}

function executingActivity(stage: StageSummary): { text: string; tone: Tone } {
  const testing = contractTesting(stage);
  if (testing !== null) return { text: testing, tone: "contract" };
  if (isContractPhase(stage)) {
    if (stage.activity_status === "Stale") {
      return stalenessText("contract writer idle", stage.staleness_secs, "warning");
    }
    if (stage.activity_status === "Quiet") {
      return stalenessText("contract writer quiet", stage.staleness_secs, "dimmed");
    }
    if (stage.activity_status === "Busy") {
      return busyText(stage, "contract writer ", "contract");
    }
    if (stage.activity_status !== "Orphaned" && stage.activity_status !== "Error") {
      return { text: "writing contract tests", tone: "contract" };
    }
  }
  switch (stage.activity_status) {
    case "Working":
      return {
        text: stage.last_tool ? `working · ${stage.last_tool}` : "working",
        tone: "completed",
      };
    case "Busy":
      return busyText(stage, "", "completed");
    case "Idle":
      return stalenessText("idle", stage.staleness_secs, "dimmed");
    case "Quiet":
      return stalenessText("quiet", stage.staleness_secs, "dimmed");
    case "Stale":
      return stalenessText("stale", stage.staleness_secs, "warning");
    case "Orphaned":
      return { text: "orphaned", tone: "blocked" };
    case "Error":
      return { text: "crashed", tone: "blocked" };
  }
}

/// `running <tool> 12m`, or `in tool call 12m` when the open call's name is
/// unknown. The silence stays in the text so a command that never returns is
/// still visible.
function busyText(stage: StageSummary, lead: string, tone: Tone): { text: string; tone: Tone } {
  const action = stage.last_tool ? `running ${stage.last_tool}` : "in tool call";
  return stalenessText(`${lead}${action}`, stage.staleness_secs, tone);
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

/// The sentence for a stage whose disputes are all ruled while the daemon runs
/// its acceptance criteria, `disputes ruled, daemon verifying 11/14 · 3m12s`;
/// null when no verdict-completion job is queued or running.
export function verdictCompleting(stage: StageSummary): string | null {
  const job = stage.verdict_completing;
  if (!job || stage.status !== "needs-adjudication") return null;
  if (job.queued) return "disputes ruled, waiting to verify";
  const elapsed = job.running_secs === null ? "" : ` · ${formatElapsed(job.running_secs)}`;
  return `disputes ruled, daemon verifying ${job.done}/${job.total}${elapsed}`;
}

function adjudicationText(stage: StageSummary): { text: string; tone: Tone } {
  const verifying = verdictCompleting(stage);
  if (verifying !== null) return { text: verifying, tone: "executing" };
  const judge =
    stage.judge_heartbeat_secs === null
      ? "none"
      : stage.judge_heartbeat_secs <= 300
        ? "working"
        : "stale";
  return { text: `dispute ${stage.dispute_count} · judge ${judge}`, tone: "warning" };
}

/// The completion blocker's sentence, `completion pending: <reason>`, for a
/// stage `loom status` lists under it; null otherwise.
export function completionActivity(stage: StageSummary): { text: string; tone: Tone } | null {
  const blocker = stage.completion_blocker;
  if (!blocker || (stage.status !== "executing" && stage.status !== "needs-human-review")) {
    return null;
  }
  const detail = blocker.summary || blocker.failure_code;
  switch (blocker.state) {
    case "pending":
      return { text: `completion pending: ${detail}`, tone: "warning" };
    case "blocked":
      return { text: `completion blocked: ${detail}`, tone: "blocked" };
    case "ownership_unknown":
      return {
        text: `completion blocked, writer unconfirmed: ${detail}`,
        tone: "blocked",
      };
  }
}

export function activityText(stage: StageSummary): { text: string; tone: Tone } | null {
  let activity = completionActivity(stage) ?? baseActivity(stage);
  if (
    !stage.completion_blocker &&
    stage.status !== "executing" &&
    stage.outgoing_session_exit_reason
  ) {
    const lastExit = `last: ${exitReasonLabel(stage.outgoing_session_exit_reason)}`;
    activity = activity
      ? { ...activity, text: `${activity.text} · ${lastExit}` }
      : { text: lastExit, tone: "neutral" };
  }
  if (!activity || !stage.held) {
    return activity;
  }
  return { text: `held · ${activity.text}`, tone: "warning" };
}

export function exitReasonLabel(reason: OutgoingSessionExitReason): string {
  const labels: Record<OutgoingSessionExitReason, string> = {
    completed: "completed",
    crashed: "crashed",
    "context-ceiling": "context ceiling",
    stalled: "stalled",
    "operator-stop": "operator stop",
    "criteria-blocked": "criteria blocked",
    replaced: "replaced",
  };
  return labels[reason];
}
