import { describe, expect, it } from "vitest";

import type { ActivityStatus, FailureType, StageStatus, StageSummary } from "@/api/schema";
import type { Tone } from "@/lib/format";
import {
  activityText,
  contractTesting,
  exitReasonLabel,
  failureLabel,
  isContractPhase,
  sessionTypeLabel,
} from "@/lib/format-activity";
import { clientFailure, stage } from "./format-kit";

describe("TUI formatter ports", () => {
  it.each<readonly [FailureType, string]>([
    ["session-crash", "crash"],
    ["test-failure", "test"],
    ["build-failure", "build"],
    ["code-error", "code"],
    ["timeout", "timeout"],
    ["context-exhausted", "context"],
    ["user-blocked", "user"],
    ["merge-conflict", "merge"],
    ["infrastructure-error", "infra"],
    ["sandbox-setup-failure", "sandbox"],
    ["startup-refusal", "startup"],
    ["unknown", "error"],
  ])("maps failure %s to %s", (failure, expected) => {
    expect(failureLabel(failure)).toBe(expected);
  });

  it.each<readonly [ActivityStatus, string, Tone]>([
    ["Working", "working · Bash", "completed"],
    ["Idle", "idle 1m1s", "dimmed"],
    ["Error", "crashed", "blocked"],
    ["Stale", "stale 1m1s", "warning"],
    ["Quiet", "quiet 1m1s", "dimmed"],
    ["Busy", "running Bash 1m1s", "completed"],
    ["Orphaned", "orphaned", "blocked"],
  ])("formats executing %s activity", (activity_status, text, tone) => {
    const result = activityText(
      stage({
        status: "executing",
        activity_status,
        last_tool: ["Working", "Busy"].includes(activity_status) ? "Bash" : null,
        staleness_secs: ["Idle", "Stale", "Quiet", "Busy"].includes(activity_status) ? 61 : null,
      }),
    );

    expect(result).toEqual({ text, tone });
  });

  it("names no tool for a busy session whose open call is unknown", () => {
    expect(
      activityText(
        stage({
          status: "executing",
          activity_status: "Busy",
          last_tool: null,
          staleness_secs: 730,
        }),
      ),
    ).toEqual({ text: "in tool call 12m10s", tone: "completed" });
  });

  it("uses the short working form when no tool is known", () => {
    expect(
      activityText(stage({ status: "executing", activity_status: "Working", last_tool: null })),
    ).toEqual({
      text: "working",
      tone: "completed",
    });
  });

  it.each<readonly [ActivityStatus, string, Tone]>([
    ["Working", "writing contract tests", "contract"],
    ["Idle", "writing contract tests", "contract"],
    ["Stale", "contract writer idle 1m1s", "warning"],
    ["Quiet", "contract writer quiet 1m1s", "dimmed"],
    ["Busy", "contract writer running Bash 1m1s", "contract"],
    ["Orphaned", "orphaned", "blocked"],
    ["Error", "crashed", "blocked"],
  ])("formats a contract-phase stage's %s activity", (activity_status, text, tone) => {
    expect(
      activityText(
        stage({
          status: "executing",
          session_type: "contract",
          activity_status,
          last_tool: ["Working", "Busy"].includes(activity_status) ? "Bash" : null,
          staleness_secs: ["Idle", "Stale", "Quiet", "Busy"].includes(activity_status) ? 61 : null,
        }),
      ),
    ).toEqual({ text, tone });
  });

  it.each<readonly [ActivityStatus, string | null]>([
    ["Working", "contracts written, testing 9/11 · 6m12s"],
    ["Quiet", "contracts written, testing 9/11 · 6m12s"],
    ["Busy", "contracts written, testing 9/11 · 6m12s"],
    ["Stale", "contracts written, testing 9/11 · 6m12s"],
    ["Orphaned", null],
    ["Error", null],
  ])("puts contract testing ahead of a %s contract writer", (activity_status, text) => {
    const testing = stage({
      status: "executing",
      session_type: "contract",
      activity_status,
      staleness_secs: 61,
      contract_testing: { done: 9, total: 11, queued: false, running_secs: 372 },
    });
    expect(contractTesting(testing)).toBe(text);
    if (text !== null) expect(activityText(testing)).toEqual({ text, tone: "contract" });
    else expect(activityText(testing)?.text).not.toContain("testing");
  });

  it("words a queued run and ignores testing outside the contract phase", () => {
    const queued = { done: 0, total: 11, queued: true, running_secs: null };
    expect(
      contractTesting(
        stage({ status: "executing", session_type: "contract", contract_testing: queued }),
      ),
    ).toBe("contracts written, waiting to test");
    expect(
      contractTesting(
        stage({ status: "executing", session_type: "stage", contract_testing: queued }),
      ),
    ).toBeNull();
    expect(contractTesting(stage({ status: "executing", session_type: "contract" }))).toBeNull();
  });

  it.each<readonly [Partial<StageSummary>, boolean]>([
    [{ status: "executing", session_type: "contract" }, true],
    [{ status: "executing", session_type: "stage" }, false],
    [{ status: "queued", session_type: "contract" }, false],
    [{ status: "executing", session_type: null }, false],
  ])("detects the contract-writer phase %#", (overrides, expected) => {
    expect(isContractPhase(stage(overrides))).toBe(expected);
  });

  it.each<readonly [StageSummary["session_type"], string | null]>([
    ["contract", "contract writer"],
    ["stage", "stage"],
    ["baseconflict", "baseconflict"],
    [null, null],
  ])("labels session type %s", (type, label) => {
    expect(sessionTypeLabel(type)).toBe(label);
  });

  it.each([
    ["stalled", "ready · last: stalled"],
    ["context-ceiling", "ready · last: context ceiling"],
  ] as const)("shows a queued stage's last %s session exit", (reason, text) => {
    expect(
      activityText(
        stage({
          status: "queued",
          outgoing_session_exit_reason: reason,
        }),
      ),
    ).toEqual({ text, tone: "queued" });
  });

  it("does not show the last session exit while executing", () => {
    expect(
      activityText(
        stage({
          status: "executing",
          activity_status: "Working",
          last_tool: null,
          outgoing_session_exit_reason: "stalled",
        }),
      ),
    ).toEqual({ text: "working", tone: "completed" });
  });

  it("shows an incoherent executing stage and lets held override its tone", () => {
    expect(
      activityText(stage({ status: "executing", incoherence: "bad frame", held: true })),
    ).toEqual({
      text: "held · incoherent",
      tone: "warning",
    });
  });

  it("prioritizes completion recovery over stale activity and incoherence", () => {
    const completion_blocker = {
      state: "pending" as const,
      fingerprint: "fp",
      failure_code: "boundary-check-failed",
      summary: "Verification did not observe the completion commit",
      commit: "abc123",
      repeat_count: 1,
      first_observed_at: null,
      last_observed_at: null,
      next_action: "wait for another verification pass",
    };

    expect(
      activityText(
        stage({
          status: "executing",
          activity_status: "Stale",
          incoherence: "stale session metadata",
          completion_blocker,
        }),
      ),
    ).toEqual({
      text: "completion pending: Verification did not observe the completion commit",
      tone: "warning",
    });
  });

  it("keeps the held prefix on a blocked completion and hides the last session exit", () => {
    expect(
      activityText(
        stage({
          status: "needs-human-review",
          held: true,
          outgoing_session_exit_reason: "context-ceiling",
          completion_blocker: {
            state: "ownership_unknown",
            fingerprint: "fp",
            failure_code: "writer-unconfirmed",
            summary: null,
            commit: "abc123",
            repeat_count: 2,
            first_observed_at: null,
            last_observed_at: null,
            next_action: "confirm commit ownership",
          },
        }),
      ),
    ).toEqual({
      text: "held · completion blocked, writer unconfirmed: writer-unconfirmed",
      tone: "warning",
    });
  });

  it("ignores completion recovery data once the stage is completed", () => {
    expect(
      activityText(
        stage({
          status: "completed",
          completion_blocker: {
            state: "blocked",
            fingerprint: "fp",
            failure_code: "old-failure",
            summary: null,
            commit: "abc123",
            repeat_count: 2,
            first_observed_at: null,
            last_observed_at: null,
            next_action: "ignore stale recovery data",
          },
        }),
      ),
    ).toBeNull();
  });

  it.each([
    ["completed", "completed"],
    ["crashed", "crashed"],
    ["context-ceiling", "context ceiling"],
    ["stalled", "stalled"],
    ["operator-stop", "operator stop"],
    ["criteria-blocked", "criteria blocked"],
    ["replaced", "replaced"],
  ] as const)("labels the %s session exit reason", (reason, label) => {
    expect(exitReasonLabel(reason)).toBe(label);
  });

  it.each<readonly [StageStatus, Partial<StageSummary>, string, Tone]>([
    ["queued", {}, "ready", "queued"],
    ["waiting-for-input", {}, "awaiting input", "warning"],
    ["needs-handoff", {}, "handing off", "warning"],
    [
      "blocked",
      { failure_info: clientFailure, retry_count: 2, max_retries: null },
      "test 2/3",
      "blocked",
    ],
    ["completed-with-failures", { retry_count: 2, max_retries: 5 }, "failed 2/5", "blocked"],
    ["merge-conflict", {}, "conflict", "warning"],
    ["merge-blocked", {}, "merge error", "blocked"],
    ["needs-human-review", { held: true }, "held · awaiting you", "warning"],
  ])("formats %s activity branches", (status, overrides, text, tone) => {
    expect(activityText(stage({ ...overrides, status }))).toEqual({ text, tone });
  });

  it.each<readonly [number | null, string]>([
    [null, "dispute 3 · judge none"],
    [300, "dispute 3 · judge working"],
    [301, "dispute 3 · judge stale"],
  ])("formats a judge heartbeat of %s", (heartbeat, text) => {
    expect(
      activityText(
        stage({
          status: "needs-adjudication",
          dispute_count: 3,
          judge_heartbeat_secs: heartbeat,
        }),
      ),
    ).toEqual({ text, tone: "warning" });
  });

  it.each<readonly [boolean, number | null, string]>([
    [false, 192, "disputes ruled, daemon verifying 11/14 · 3m12s"],
    [true, null, "disputes ruled, waiting to verify"],
  ])("reports the daemon verifying ruled disputes (queued %s)", (queued, running, text) => {
    expect(
      activityText(
        stage({
          status: "needs-adjudication",
          dispute_count: 4,
          judge_heartbeat_secs: 30,
          verdict_completing: { done: 11, total: 14, queued, running_secs: running },
        }),
      ),
    ).toEqual({ text, tone: "executing" });
  });

  it("leaves completed activity blank", () => {
    expect(activityText(stage({ status: "completed" }))).toBeNull();
  });
});
