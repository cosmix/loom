import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import {
  snapshotSchema,
  type ActivityStatus,
  type FailureType,
  type StageStatus,
  type StageSummary,
} from "@/api/schema";
import {
  activityText,
  contextUsage,
  daemonLine,
  failureLabel,
  formatElapsed,
  LEGEND,
  mergeText,
  modelsOf,
  progressPercent,
  stateMeta,
  summaryCounts,
  timeText,
  type Tone,
} from "@/lib/format";
import { LEGEND as SHARED_LEGEND } from "@/lib/states";

const fixture = snapshotSchema.parse(fixtureJson);
const stages = new Map(fixture.status.stages.map((stage) => [stage.id, stage]));
const template = fixture.status.stages[0];
const clientFailure = stages.get("client")!.failure_info!;

function stage(overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...template, ...overrides };
}

describe("TUI formatter ports", () => {
  it.each([
    [0, "0s"],
    [59, "59s"],
    [60, "1m0s"],
    [3599, "59m59s"],
    [3600, "1h0m"],
  ])("formats %i seconds as %s", (seconds, expected) => {
    expect(formatElapsed(seconds)).toBe(expected);
  });

  it.each<readonly [StageStatus, string, string, Tone, boolean]>([
    ["waiting-for-deps", "○", "Waiting", "pending", false],
    ["queued", "▶", "Queued", "queued", true],
    ["executing", "●", "Executing", "executing", true],
    ["waiting-for-input", "?", "Input", "warning", true],
    ["blocked", "✗", "Blocked", "blocked", true],
    ["completed", "✓", "Completed", "completed", true],
    ["needs-handoff", "⟳", "Handoff", "warning", true],
    ["skipped", "⊘", "Skipped", "dimmed", false],
    ["merge-conflict", "⚡", "Conflict", "warning", true],
    ["completed-with-failures", "⚠", "Failed", "blocked", true],
    ["merge-blocked", "⊗", "MergeBlk", "blocked", true],
    ["needs-human-review", "⏸", "Review", "warning", false],
    ["needs-adjudication", "⚖", "Adjudicate", "warning", true],
  ])("maps %s to its TUI state metadata", (status, icon, label, tone, bold) => {
    expect(stateMeta(status)).toEqual({ icon, label, tone, bold });
  });

  it("keeps the formatter legend in lockstep with the shared status table", () => {
    expect(LEGEND).toEqual(
      SHARED_LEGEND.map(({ status, legend }) => ({ status, meaning: legend })),
    );
  });

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

  it.each([
    [0, 0, "green"],
    [0.59, 2, "green"],
    [0.6, 3, "yellow"],
    [0.89, 4, "yellow"],
    [0.9, 4, "red"],
    [1, 5, "red"],
  ] as const)("uses the correct %s context band", (ratio, filled, health) => {
    const reading = contextUsage(
      stage({
        status: "executing",
        context_tokens: ratio * 100,
        context_ceiling_tokens: 100,
      }),
    );

    expect(reading).toMatchObject({ percent: ratio * 100, filled, health });
  });

  it("requires an active status, tokens, and a non-zero ceiling", () => {
    expect(
      contextUsage(stage({ status: "queued", context_tokens: 1, context_ceiling_tokens: 1 })),
    ).toBeNull();
    expect(contextUsage(stage({ status: "executing", context_tokens: null }))).toBeNull();
    expect(
      contextUsage(stage({ status: "executing", context_tokens: 1, context_ceiling_tokens: 0 })),
    ).toBeNull();
  });

  it.each<readonly [ActivityStatus, string, Tone]>([
    ["Working", "working · Bash", "completed"],
    ["Idle", "idle 1m1s", "dimmed"],
    ["Error", "crashed", "blocked"],
    ["Stale", "stale 1m1s", "warning"],
    ["Orphaned", "orphaned", "blocked"],
  ])("formats executing %s activity", (activity_status, text, tone) => {
    const result = activityText(
      stage({
        status: "executing",
        activity_status,
        last_tool: activity_status === "Working" ? "Bash" : null,
        staleness_secs: ["Idle", "Stale"].includes(activity_status) ? 61 : null,
      }),
    );

    expect(result).toEqual({ text, tone });
  });

  it("uses the short working form when no tool is known", () => {
    expect(
      activityText(stage({ status: "executing", activity_status: "Working", last_tool: null })),
    ).toEqual({
      text: "working",
      tone: "completed",
    });
  });

  it("shows an incoherent executing stage and lets held override its tone", () => {
    expect(
      activityText(stage({ status: "executing", incoherence: "bad frame", held: true })),
    ).toEqual({
      text: "held · incoherent",
      tone: "warning",
    });
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

  it("leaves completed activity blank", () => {
    expect(activityText(stage({ status: "completed" }))).toBeNull();
  });

  it.each<readonly [Partial<StageSummary>, { text: string; tone: Tone } | null]>([
    [
      { status: "completed", stage_type: "standard", merged: true },
      { text: "merged", tone: "merged" },
    ],
    [
      { status: "completed", stage_type: "standard", merged: false },
      { text: "unmerged", tone: "warning" },
    ],
    [{ status: "merge-conflict" }, { text: "conflict", tone: "warning" }],
    [{ status: "merge-blocked" }, { text: "error", tone: "blocked" }],
    [
      { status: "completed", stage_type: "knowledge", cleanup_warning: "leftover" },
      { text: "cleanup!", tone: "warning" },
    ],
    [{ status: "completed", stage_type: "knowledge", merged: true }, null],
  ])("formats merge state %#", (overrides, expected) => {
    expect(mergeText(stage(overrides))).toEqual(expected);
  });

  it.each<readonly [Partial<StageSummary>, string | null]>([
    [{ status: "waiting-for-deps" }, null],
    [{ status: "queued" }, null],
    [{ status: "completed", execution_secs: 90, elapsed_secs: 5 }, "1m30s"],
    [{ status: "completed", execution_secs: null, elapsed_secs: 61 }, "1m1s"],
    [{ status: "completed", execution_secs: null, elapsed_secs: null }, ""],
  ])("formats time state %#", (overrides, expected) => {
    expect(timeText(stage(overrides))).toBe(expected);
  });

  it("preserves the primary and execution model lists", () => {
    expect(modelsOf(stages.get("server")!)).toEqual({
      model: "opus",
      execution: ["sonnet", "gpt-5.6-terra"],
    });
  });

  it.each([
    [1, 7, 14],
    [2, 3, 67],
    [0, 0, 0],
  ])("rounds %i of %i progress to %i percent", (completed, total, expected) => {
    expect(progressPercent(completed, total)).toBe(expected);
  });

  it.each([
    ["running", 75, { text: "loop stalled 75s", tone: "warning" }],
    ["unreachable", 59, { text: "daemon running", detail: "tick 59s ago", tone: "completed" }],
    ["running", null, { text: "daemon running", detail: "tick unknown", tone: "completed" }],
    ["process-only", 4, { text: "daemon process alive, socket missing", tone: "warning" }],
    ["not-running", 4, { text: "daemon stopped", tone: "dimmed" }],
  ] as const)("formats daemon %s", (daemon, age, expected) => {
    expect(daemonLine(daemon, age)).toEqual(expected);
  });

  it("counts the same summary statuses as the TUI header", () => {
    expect(summaryCounts(fixture.status.stages, fixture.attention.length)).toEqual({
      executing: 1,
      queued: 0,
      waiting: 2,
      attention: 3,
      done: 1,
    });
  });
});
