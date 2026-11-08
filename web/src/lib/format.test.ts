import { describe, expect, it } from "vitest";

import { snapshotSchema, type Busy, type StageStatus, type StageSummary } from "@/api/schema";
import {
  contextUsage,
  daemonLine,
  formatClock,
  formatElapsed,
  formatStamp,
  mergeText,
  modelsOf,
  progressPercent,
  stateMeta,
  summaryCounts,
  timeText,
  type Tone,
} from "@/lib/format";
import { fixture, stage, stages } from "./format-kit";
import fixtureJson from "@/api/fixtures/snapshot.json";

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

  it.each<readonly [StageStatus, Tone, boolean]>([
    ["waiting-for-deps", "pending", false],
    ["queued", "queued", true],
    ["executing", "executing", true],
    ["waiting-for-input", "warning", true],
    ["blocked", "blocked", true],
    ["completed", "completed", true],
    ["needs-handoff", "warning", true],
    ["skipped", "dimmed", false],
    ["merge-conflict", "warning", true],
    ["completed-with-failures", "blocked", true],
    ["merge-blocked", "blocked", true],
    ["needs-human-review", "warning", false],
    ["needs-adjudication", "warning", true],
  ])("maps %s to its TUI state metadata", (status, tone, bold) => {
    expect(stateMeta(status)).toEqual({ tone, bold });
  });

  it.each([
    [0, 0, "green"],
    [0.09, 0, "green"],
    [0.1, 1, "green"],
    [0.39, 2, "green"],
    [0.59, 3, "green"],
    [0.6, 3, "yellow"],
    [0.89, 4, "yellow"],
    [0.9, 5, "red"],
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

  it.each<readonly [Partial<StageSummary>, { text: string; tone: Tone } | null]>([
    [
      { status: "completed", stage_type: "standard", merged: true },
      { text: "merged", tone: "merged" },
    ],
    [
      { status: "completed", stage_type: "standard", merged: true, merge_assumed: true },
      { text: "assumed", tone: "merged" },
    ],
    [
      { status: "completed", stage_type: "standard", merged: false },
      { text: "unmerged", tone: "warning" },
    ],
    [{ status: "merge-conflict" }, { text: "conflict", tone: "warning" }],
    [{ status: "merge-blocked" }, { text: "error", tone: "blocked" }],
    [
      { status: "merge-blocked", merge_block: "the target moved" },
      { text: "blocked", tone: "blocked" },
    ],
    [
      { status: "completed", stage_type: "standard", merged: true, stash_warning: "refs/x" },
      { text: "stash!", tone: "blocked" },
    ],
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
    [{ status: "completed", execution_secs: 90, elapsed_secs: 150 }, "2m30s"],
    [{ status: "completed-with-failures", execution_secs: 90, elapsed_secs: 150 }, "2m30s"],
    [{ status: "completed", execution_secs: 90, elapsed_secs: null }, "1m30s"],
    [{ status: "completed", execution_secs: 90, elapsed_secs: 0 }, "0s"],
    [{ status: "executing", execution_secs: 90, elapsed_secs: 150 }, "1m30s"],
    [{ status: "completed", execution_secs: null, elapsed_secs: 61 }, "1m1s"],
    [{ status: "completed", execution_secs: null, elapsed_secs: null }, ""],
  ])("formats time state %#", (overrides, expected) => {
    expect(timeText(stage(overrides))).toBe(expected);
  });

  it("preserves the primary and execution model lists", () => {
    expect(modelsOf(stages.get("server")!)).toEqual({
      model: "opus",
      execution: ["sonnet", "gpt-6.1-terra"],
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
    ["running", 75, null, { text: "loop stalled 1m15s", tone: "warning" }],
    ["running", null, null, { text: "daemon running", detail: "tick unknown", tone: "completed" }],
    ["running", 59, null, { text: "daemon running", detail: "tick 59s ago", tone: "completed" }],
    [
      "unreachable",
      59,
      null,
      { text: "daemon running", detail: "socket unreachable", tone: "warning" },
    ],
    ["unreachable", 75, null, { text: "loop stalled 1m15s", tone: "warning" }],
    ["process-only", 4, null, { text: "daemon process alive, socket missing", tone: "warning" }],
    ["not-running", 4, null, { text: "daemon stopped", tone: "dimmed" }],
    ["running", 2, 130, { text: "daemon unknown", detail: "no data for 2m10s", tone: "dimmed" }],
    ["unreachable", null, 5, { text: "daemon unknown", detail: "no data for 5s", tone: "dimmed" }],
    ["process-only", null, 0, { text: "daemon unknown", detail: "no data for 0s", tone: "dimmed" }],
    [
      "not-running",
      null,
      45,
      { text: "daemon unknown", detail: "no data for 45s", tone: "dimmed" },
    ],
  ] as const)("formats daemon %s (tick %s, stale %s)", (daemon, tickAge, staleSecs, expected) => {
    expect(daemonLine(daemon, tickAge, staleSecs, null, Date.now())).toEqual(expected);
  });

  describe("daemonLine with a busy operation", () => {
    const now = Date.parse("2026-11-08T12:00:00.000Z");
    const busy: Busy = {
      op: "merge-verification",
      what: "verifying the merge of stage-a",
      stage_id: "stage-a",
      started_at: new Date(now - 90_000).toISOString(),
      ceiling_secs: 1200,
      global_ceiling_secs: 1800,
    };

    it("never shows a leftover busy line for a stopped or half-alive daemon", () => {
      expect(daemonLine("not-running", 2, null, busy, now)).toEqual({
        text: "daemon stopped",
        tone: "dimmed",
      });
      expect(daemonLine("process-only", 2, null, busy, now)).toEqual({
        text: "daemon process alive, socket missing",
        tone: "warning",
      });
    });

    it("prints the tick age, not the operation's elapsed time, for a stale tick", () => {
      expect(daemonLine("running", 75, null, busy, now)).toEqual({
        text: "loop stalled 1m15s",
        tone: "warning",
      });
    });

    it("keeps the unreachable detail ahead of the busy detail", () => {
      expect(daemonLine("unreachable", 2, null, busy, now)).toEqual({
        text: "daemon running",
        detail: "socket unreachable",
        tone: "warning",
      });
    });

    it("shows the operation and its elapsed time while the tick is fresh", () => {
      expect(daemonLine("running", 2, null, busy, now)).toEqual({
        text: "daemon running",
        detail: "busy verifying the merge of stage-a 1m30s",
        tone: "completed",
      });
    });

    it("treats an unparseable started_at as just started", () => {
      const line = daemonLine("running", 2, null, { ...busy, started_at: "garbage" }, now);
      expect(line.detail).toBe("busy verifying the merge of stage-a 0s");
    });
  });

  describe("snapshot busy field", () => {
    const wire = {
      op: "merge-verification",
      what: "verifying the merge of stage-a",
      stage_id: "stage-a",
      started_at: "2026-11-07T13:51:08.120Z",
      ceiling_secs: 1200,
      global_ceiling_secs: 1800,
    };

    it("round-trips the daemon's busy object", () => {
      const parsed = snapshotSchema.parse({ ...fixtureJson, busy: wire });
      expect(parsed.busy).toEqual(wire);
    });

    it("parses a snapshot without the key to busy null", () => {
      const { busy: _omitted, ...withoutBusy } = { ...fixtureJson, busy: null };
      expect(snapshotSchema.parse(withoutBusy).busy).toBeNull();
    });

    it("rejects an unknown operation", () => {
      expect(
        snapshotSchema.safeParse({ ...fixtureJson, busy: { ...wire, op: "napping" } }).success,
      ).toBe(false);
    });
  });

  describe("formatClock / formatStamp", () => {
    it("renders a 24-hour clock", () => {
      expect(formatClock(0)).toMatch(/^\d{2}:\d{2}:\d{2}$/);
    });

    it("renders a 24-hour date and time, never AM/PM", () => {
      const result = formatStamp(0);
      expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
      expect(result).not.toMatch(/[AP]M/i);
    });

    it.each([formatClock, formatStamp])("returns — for an unparseable input", (fn) => {
      expect(fn("not a date")).toBe("—");
      expect(fn(Number.NaN)).toBe("—");
    });
  });

  it("counts the same summary statuses as the TUI header", () => {
    expect(summaryCounts(fixture.status.stages, fixture.attention.length)).toEqual({
      executing: 1,
      queued: 0,
      waiting: 2,
      attention: 5,
      done: 1,
    });
  });
});
