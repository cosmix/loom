import type { AgentFlow, CodexJob } from "@/api/schema";
import type { Tone } from "@/lib/format";

const CODEX_ID_PREFIX = "codex-";

/// The first characters of a Codex thread id that a codex agent id carries
/// (`codex-01a1e59a` is thread `01a1e59a-...`); null for any other agent.
function threadPrefix(agent: AgentFlow): string | null {
  if (agent.kind !== "codex" || !agent.id.startsWith(CODEX_ID_PREFIX)) return null;
  const prefix = agent.id.slice(CODEX_ID_PREFIX.length);
  return prefix === "" ? null : prefix;
}

/// Each codex agent's native worker invocation, keyed by agent id: the job
/// whose thread starts with the agent id's suffix, as the terminal view joins
/// them. An agent with no such job has none: a unit retried under one id keeps
/// one job per lane, and a lane never borrows an older attempt's outcome.
export function matchCodexJobs(
  jobs: readonly CodexJob[],
  agents: readonly AgentFlow[],
): Map<string, CodexJob> {
  const matched = new Map<string, CodexJob>();
  const claimed = new Set<CodexJob>();
  for (const agent of agents) {
    const prefix = threadPrefix(agent);
    if (prefix === null) continue;
    const job = jobs.find(
      (candidate) => candidate.thread_id?.startsWith(prefix) && !claimed.has(candidate),
    );
    if (job) {
      matched.set(agent.id, job);
      claimed.add(job);
    }
  }
  return matched;
}

/// The jobs no lane claimed: a unit that failed before a Codex thread started
/// has no token agent, and this keeps it visible.
export function unmatchedCodexJobs(
  jobs: readonly CodexJob[],
  matched: ReadonlyMap<string, CodexJob>,
): CodexJob[] {
  const claimed = new Set(matched.values());
  return jobs.filter((job) => !claimed.has(job));
}

export interface JobStatus {
  text: string;
  tone: Tone;
  /// The outcome's explanation, shown as the chip's tooltip.
  reason: string | undefined;
}

/// The chip for a job: its outcome once there is one, else "running", or
/// "overdue" once a job still running has outlasted its deadline. A job at its
/// terminal phase with no outcome reads as unknown. `now` is epoch milliseconds.
export function jobStatus(job: CodexJob, now: number): JobStatus {
  const reason = job.outcome_reason ?? undefined;
  switch (job.outcome) {
    case "succeeded":
      return { text: "succeeded", tone: "completed", reason };
    case "failed":
      return { text: "failed", tone: "blocked", reason };
    case "cancelled":
      return { text: "cancelled", tone: "warning", reason };
    case "timed_out":
      return { text: "timed out", tone: "blocked", reason };
    case "unknown":
      return { text: "unknown", tone: "dimmed", reason };
    case null: {
      if (job.phase === "terminal") return { text: "unknown", tone: "dimmed", reason };
      const secs = jobRunSecs(job, now);
      return secs !== null && secs > job.deadline_secs
        ? { text: "overdue", tone: "warning", reason }
        : { text: "running", tone: "executing", reason };
    }
  }
}

/// Whole seconds the job has run: to `ended_at` once terminal, else to `now`
/// (epoch milliseconds). Null when a timestamp does not parse; never negative.
export function jobRunSecs(job: CodexJob, now: number): number | null {
  const started = Date.parse(job.started_at);
  const ended = job.ended_at === null ? now : Date.parse(job.ended_at);
  if (Number.isNaN(started) || Number.isNaN(ended)) return null;
  return Math.max(0, Math.floor((ended - started) / 1000));
}

const pad2 = (value: number): string => String(value).padStart(2, "0");

/// A run time as the terminal view prints it: `23s`, `4m10s`, `1h02m`.
export function formatJobDuration(seconds: number): string {
  const secs = Math.max(0, Math.floor(seconds));
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m${pad2(secs % 60)}s`;
  return `${Math.floor(secs / 3600)}h${pad2(Math.floor((secs % 3600) / 60))}m`;
}

/// A deadline as the terminal view prints it: `25m` for whole minutes under an
/// hour, else as [formatJobDuration] (`1m30s`, `1h00m`).
export function formatDeadline(seconds: number): string {
  return seconds >= 60 && seconds < 3600 && seconds % 60 === 0
    ? `${seconds / 60}m`
    : formatJobDuration(seconds);
}

/// `23s / 25m`: the run time against the deadline; `— / 25m` when it cannot be told.
export function jobTimingText(job: CodexJob, now: number): string {
  const secs = jobRunSecs(job, now);
  return `${secs === null ? "—" : formatJobDuration(secs)} / ${formatDeadline(job.deadline_secs)}`;
}
