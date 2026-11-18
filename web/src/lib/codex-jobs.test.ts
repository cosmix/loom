import { describe, expect, it } from "vitest";

import type { AgentFlow, CodexJob } from "@/api/schema";
import {
  formatDeadline,
  formatJobDuration,
  jobRunSecs,
  jobStatus,
  jobTimingText,
  matchCodexJobs,
  unmatchedCodexJobs,
} from "@/lib/codex-jobs";
import { modelWithEffort } from "@/lib/format";

const ZERO = { input: 0, cache_write: 0, cache_read: 0, output: 0 };

function agent(id: string, unitId: string | null, kind: AgentFlow["kind"] = "codex"): AgentFlow {
  return {
    id,
    kind,
    agent_type: null,
    forwarder: null,
    model: "gpt-6.1-terra",
    effort: "xhigh",
    unit_id: unitId,
    active: false,
    total: ZERO,
    rate: ZERO,
    series: [],
  };
}

function job(unitId: string, threadId: string | null, patch: Partial<CodexJob> = {}): CodexJob {
  return {
    unit_id: unitId,
    invocation_id: `inv-${unitId}-${threadId ?? "none"}`,
    model: "gpt-6.1-terra",
    effort: "xhigh",
    phase: "terminal",
    outcome: "succeeded",
    outcome_reason: "turn completed",
    thread_id: threadId,
    started_at: "2026-11-16T18:23:55.000Z",
    ended_at: "2026-11-16T18:24:18.000Z",
    deadline_secs: 1500,
    ...patch,
  };
}

describe("matchCodexJobs", () => {
  it("joins a lane to the job whose thread starts with the agent id suffix", () => {
    const threaded = job("hex-test", "01a1e59a-099e-7ef2");
    const matched = matchCodexJobs([threaded], [agent("codex-01a1e59a", null)]);
    expect(matched.get("codex-01a1e59a")).toBe(threaded);
  });

  it("leaves a lane without a thread match unjoined, whatever its unit id", () => {
    const older = job("hex-test", "ffff0000-0000", { outcome: "failed" });
    const matched = matchCodexJobs([older], [agent("codex-01a1e59a", "hex-test")]);
    expect(matched.size).toBe(0);
  });

  it("gives a retried unit one job per lane instead of the first job twice", () => {
    const first = job("hex-test", "aaaa1111-0");
    const retry = job("hex-test", "bbbb2222-0");
    const matched = matchCodexJobs(
      [first, retry],
      [agent("codex-bbbb2222", "hex-test"), agent("codex-aaaa1111", "hex-test")],
    );
    expect(matched.get("codex-aaaa1111")).toBe(first);
    expect(matched.get("codex-bbbb2222")).toBe(retry);
  });

  it("never joins a non-codex agent", () => {
    const matched = matchCodexJobs(
      [job("hex-test", null)],
      [agent("main-01a1e59a", "hex-test", "main")],
    );
    expect(matched.size).toBe(0);
  });
});

describe("unmatchedCodexJobs", () => {
  it("returns the thread-less job no lane claimed, in order", () => {
    const threaded = job("hex-test", "01a1e59a-0");
    const threadless = job("hex-lint", null, { outcome: "failed" });
    const jobs = [threaded, threadless];
    const matched = matchCodexJobs(jobs, [agent("codex-01a1e59a", "hex-test")]);
    expect(unmatchedCodexJobs(jobs, matched)).toEqual([threadless]);
  });

  it("returns every job when there are no lanes", () => {
    const jobs = [job("a", null), job("b", null)];
    expect(unmatchedCodexJobs(jobs, matchCodexJobs(jobs, []))).toEqual(jobs);
  });
});

describe("jobStatus", () => {
  it.each([
    ["succeeded", "succeeded", "completed"],
    ["failed", "failed", "blocked"],
    ["cancelled", "cancelled", "warning"],
    ["timed_out", "timed out", "blocked"],
    ["unknown", "unknown", "dimmed"],
  ] as const)("reads outcome %s as %s", (outcome, text, tone) => {
    expect(jobStatus(job("u", null, { outcome }), 0)).toEqual({
      text,
      tone,
      reason: "turn completed",
    });
  });

  it("reads a job without an outcome as running until it is terminal", () => {
    const running = job("u", "t", { outcome: null, outcome_reason: null, phase: "running" });
    const now = Date.parse(running.started_at) + 130_000;
    expect(jobStatus(running, now)).toEqual({
      text: "running",
      tone: "executing",
      reason: undefined,
    });
    expect(jobStatus({ ...running, phase: "terminal" }, now).text).toBe("unknown");
  });

  it("reads a running job past its deadline as overdue, a finished one by its outcome", () => {
    const started = Date.parse("2026-11-16T18:23:55.000Z");
    const running = job("u", "t", { outcome: null, ended_at: null, phase: "running" });
    expect(jobStatus(running, started + 1_500_000).text).toBe("running");
    expect(jobStatus(running, started + 1_501_000)).toMatchObject({
      text: "overdue",
      tone: "warning",
    });
    const late = job("u", "t", { ended_at: "2026-11-16T18:53:55.000Z" });
    expect(jobStatus(late, started + 9_999_000).text).toBe("succeeded");
  });
});

describe("job timing", () => {
  const started = Date.parse("2026-11-16T18:23:55.000Z");

  it("measures a finished job to its end time, ignoring now", () => {
    expect(jobRunSecs(job("u", "t"), started + 999_000)).toBe(23);
    expect(jobTimingText(job("u", "t"), started + 999_000)).toBe("23s / 25m");
  });

  it("measures a running job to now", () => {
    const running = job("u", "t", { ended_at: null, outcome: null, phase: "running" });
    expect(jobTimingText(running, started + 250_000)).toBe("4m10s / 25m");
  });

  it("never goes negative on clock skew and survives an unparseable stamp", () => {
    const running = job("u", "t", { ended_at: null });
    expect(jobRunSecs(running, started - 5000)).toBe(0);
    expect(jobRunSecs(job("u", "t", { started_at: "garbage" }), started)).toBeNull();
    expect(jobTimingText(job("u", "t", { started_at: "garbage" }), started)).toBe("— / 25m");
  });

  // The same table as `durations_and_deadlines_read_the_same_in_both_views`
  // in the terminal view's tests.
  it.each([
    [0, "0s", "0s"],
    [23, "23s", "23s"],
    [90, "1m30s", "1m30s"],
    [1500, "25m00s", "25m"],
    [3600, "1h00m", "1h00m"],
    [3661, "1h01m", "1h01m"],
  ])("reads %i s as %s and as the deadline %s", (seconds, duration, deadline) => {
    expect(formatJobDuration(seconds)).toBe(duration);
    expect(formatDeadline(seconds)).toBe(deadline);
  });
});

it("modelWithEffort appends the effort when known", () => {
  expect(modelWithEffort("sonnet", "high")).toBe("sonnet/high");
  expect(modelWithEffort("sonnet", null)).toBe("sonnet");
  expect(modelWithEffort("sonnet", "")).toBe("sonnet");
});
