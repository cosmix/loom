import type { Attention, IssueKind, StageIssue, StageStatus, StageSummary } from "@/api/schema";
import { attentionStatus } from "@/components/attention-status";
import { stateMeta, type Tone } from "@/lib/format";

/// How many events the history card shows before "Show all".
export const RECENT_EVENTS = 6;

/// Where an event stands: the stage's live issue, or how an archived one ended.
export type HistoryMark =
  | "current"
  | "resolved"
  | "closed"
  | "superseded"
  | "reset"
  | "retried"
  | "rejected"
  | "skipped"
  | "accepted"
  | "needs-evidence"
  | "ruled";

/// One row of the history card.
export interface HistoryEvent {
  key: string;
  /// The state whose glyph the tag wears.
  status: StageStatus;
  tone: Tone;
  tag: string;
  title: string;
  /// The time column: when an archived issue ended, when the live one began.
  at: string | null;
  detectedAt: string | null;
  evidence: string[];
  /// `ended_by` verbatim; null for the live issue.
  endedBy: string | null;
  mark: HistoryMark;
}

const KIND: Record<IssueKind, { status: StageStatus; tag: string; tone: Tone }> = {
  failure: { status: "blocked", tag: "failure", tone: "blocked" },
  review: { status: "needs-human-review", tag: "review", tone: "warning" },
  "merge-conflict": { status: "merge-conflict", tag: "merge", tone: "warning" },
  adjudication: { status: "needs-adjudication", tag: "adjudication", tone: "contract" },
};

/// The issue kind a live state archives as, when it ends.
const STATUS_KIND: Partial<Record<StageStatus, IssueKind>> = {
  blocked: "failure",
  "completed-with-failures": "failure",
  "merge-conflict": "merge-conflict",
  "needs-human-review": "review",
  "needs-adjudication": "adjudication",
};

/// States whose `failure_info` describes the issue the stage is in now. An
/// adjudication rules on the failure that raised it, so that failure stays
/// current (and on the attention entry) until the verdict lands.
const CURRENT_FAILURE_STATES = new Set<StageStatus>([
  "blocked",
  "completed-with-failures",
  "merge-conflict",
  "merge-blocked",
  "needs-human-review",
  "needs-adjudication",
]);

/// The stage's live failure, or null when `failure_info` is an earlier
/// attempt's leftover.
export function currentFailure(stage: StageSummary): StageSummary["failure_info"] {
  return CURRENT_FAILURE_STATES.has(stage.status) ? stage.failure_info : null;
}

/// How a transition `<From> -> <To>` (`retire_issues_of` in
/// `loom/src/models/stage/issue_history.rs`, `StageStatus`'s `Display` names)
/// ended an issue: back to waiting is a reset, a failed stage queued or run
/// again is a retry, a conflict replaces the failure before it, and any other
/// way out resolved it.
function transitionMark(from: string, to: string): HistoryMark {
  if (to === "WaitingForDeps") return "reset";
  if (to === "Skipped") return "skipped";
  if (to === "MergeConflict") return "superseded";
  const failed = from === "Blocked" || from === "CompletedWithFailures";
  return failed && (to === "Queued" || to === "Executing") ? "retried" : "resolved";
}

/// The ruling word an applied verdict ends a dispute with
/// (`adjudicator verdict: <ruling>`, `DisputeVerdict`'s kebab-case tags).
const RULING_MARK: Record<string, HistoryMark> = {
  accept: "accepted",
  reject: "rejected",
  "needs-more-evidence": "needs-evidence",
  rulings: "ruled",
};

/// The `ended_by` strings loom writes besides a transition: the commands and
/// paths that retire an issue (`retire_failure`, `retire_review_reason`) and
/// the endings of an adjudication round (`commands/status/data/rounds.rs`).
const ENDED_BY_MARK: [prefix: string, mark: HistoryMark][] = [
  ["loom stage reset", "reset"],
  ["loom stage retry", "retried"],
  ["replaced by ", "superseded"],
  ["blocked by loom stage block", "superseded"],
  ["human review rejected", "rejected"],
  ["closed ", "closed"],
  ["ended without ", "closed"],
];

export function endMark(endedBy: string): HistoryMark {
  const [transition, from = "", to = ""] = /^(\w+) -> (\w+)$/.exec(endedBy) ?? [];
  if (transition) return transitionMark(from, to);
  const [verdict, ruling = ""] = /^adjudicator verdict: (\S+)$/.exec(endedBy) ?? [];
  if (verdict) return RULING_MARK[ruling] ?? "ruled";
  return ENDED_BY_MARK.find(([prefix]) => endedBy.startsWith(prefix))?.[1] ?? "resolved";
}

function sentence(text: string): string {
  const lower = text.toLowerCase();
  return lower.charAt(0).toUpperCase() + lower.slice(1);
}

function archivedTitle(issue: StageIssue): string {
  switch (issue.kind) {
    case "failure":
      return issue.failure_type ? sentence(issue.failure_type.replaceAll("-", " ")) : "Failure";
    case "review":
      return "Human review";
    case "merge-conflict":
      return "Merge conflict";
    case "adjudication":
      return issue.dispute_id === null ? "Adjudication round" : `Dispute ${issue.dispute_id}`;
  }
}

/// What identifies an archived issue across snapshots: when and how it
/// ended, and its dispute. Stable as newer issues arrive on top.
function issueKey(issue: StageIssue): string {
  return `${issue.ended_at}|${issue.kind}|${issue.dispute_id ?? ""}|${issue.ended_by}`;
}

function archivedEvent(issue: StageIssue): HistoryEvent {
  return {
    key: issueKey(issue),
    ...KIND[issue.kind],
    title: archivedTitle(issue),
    at: issue.ended_at,
    detectedAt: issue.detected_at,
    evidence: issue.evidence,
    endedBy: issue.ended_by,
    mark: endMark(issue.ended_by),
  };
}

/// The live issue, from the attention entry: its label, the failure it
/// carries, and when that failure was detected. It wears the entry's tone, as
/// the now card and the attention card do; its specifics are the now card's,
/// so it brings no evidence of its own.
function currentEvent(stage: StageSummary, entry: Attention): HistoryEvent {
  const kind = STATUS_KIND[stage.status];
  const status = attentionStatus(entry);
  const label = sentence(entry.label);
  return {
    key: "current",
    status,
    tone: stateMeta(status).tone,
    tag: kind === undefined ? "attention" : KIND[kind].tag,
    title: entry.failure_label ? `${label} · ${entry.failure_label}` : label,
    at: currentFailure(stage)?.detected_at ?? null,
    detectedAt: null,
    evidence: [],
    endedBy: null,
    mark: "current",
  };
}

/// Every event the payload holds for the stage, newest first: the live issue
/// on top, then the archived ones in the order the daemon sent them. Two
/// issues that ended alike in the same instant get distinct keys.
export function historyEvents(stage: StageSummary, entry: Attention | undefined): HistoryEvent[] {
  const seen = new Map<string, number>();
  const archived = stage.history.map((issue) => {
    const event = archivedEvent(issue);
    const repeat = seen.get(event.key) ?? 0;
    seen.set(event.key, repeat + 1);
    return repeat === 0 ? event : { ...event, key: `${event.key}#${repeat}` };
  });
  return entry ? [currentEvent(stage, entry), ...archived] : archived;
}

const NOUNS: Record<string, [one: string, many: string]> = {
  failure: ["failure", "failures"],
  review: ["review", "reviews"],
  merge: ["merge conflict", "merge conflicts"],
  adjudication: ["adjudication round", "adjudication rounds"],
  attention: ["current issue", "current issues"],
};

/// "2 adjudication rounds · 1 failure · 1 merge conflict", in first-seen order.
export function historyCounts(events: HistoryEvent[]): string {
  const counts = new Map<string, number>();
  for (const event of events) counts.set(event.tag, (counts.get(event.tag) ?? 0) + 1);
  return [...counts]
    .map(([tag, count]) => {
      const [one, many] = NOUNS[tag] ?? [tag, tag];
      return `${count} ${count === 1 ? one : many}`;
    })
    .join(" · ");
}

/// Whether any event is a failure: a stage blocked or completed with failures
/// now, or an archived issue of kind failure. Reviews, merge conflicts and
/// adjudications are not.
export function hasFailure(events: HistoryEvent[]): boolean {
  return events.some((event) => event.tag === KIND.failure.tag);
}
