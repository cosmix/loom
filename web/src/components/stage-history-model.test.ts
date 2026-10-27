import { describe, expect, it } from "vitest";

import { endMark } from "@/components/stage-history-model";

// Every `ended_by` loom writes: a transition label `<From> -> <To>` from
// `retire_issues_of`, a command or path passed to `retire_failure` and
// `retire_review_reason`, or an adjudication round's ending (`rounds.rs`).
describe("endMark", () => {
  it.each([
    ["Blocked -> WaitingForDeps", "reset"],
    ["loom stage reset", "reset"],
    ["Blocked -> Queued", "retried"],
    ["CompletedWithFailures -> Queued", "retried"],
    ["Blocked -> Executing", "retried"],
    ["loom stage retry --force", "retried"],
    ["NeedsHumanReview -> Queued", "resolved"],
    ["MergeBlocked -> Queued", "resolved"],
    ["MergeConflict -> Completed", "resolved"],
    ["NeedsHumanReview -> Completed", "resolved"],
    ["merge block cleared", "resolved"],
    ["adjudicator verdict applied", "resolved"],
    ["Blocked -> Skipped", "skipped"],
    ["MergeBlocked -> MergeConflict", "superseded"],
    ["replaced by a newer failure", "superseded"],
    ["replaced by a newer review reason", "superseded"],
    ["blocked by loom stage block", "superseded"],
    ["human review rejected", "rejected"],
    ["adjudicator verdict: accept", "accepted"],
    ["adjudicator verdict: reject", "rejected"],
    ["adjudicator verdict: needs-more-evidence", "needs-evidence"],
    ["adjudicator verdict: rulings", "ruled"],
    ["closed without a verdict", "closed"],
    ["closed before its verdict was applied", "closed"],
    ["ended without a readable verdict", "closed"],
  ])("reads %s as %s", (endedBy, mark) => {
    expect(endMark(endedBy)).toBe(mark);
  });
});
