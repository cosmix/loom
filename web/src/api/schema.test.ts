import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, stageStatusSchema } from "@/api/schema";

const validBlocker = {
  state: "pending",
  fingerprint: "fp",
  failure_code: "boundary-check-failed",
  summary: null,
  commit: "abc123",
  repeat_count: 1,
  first_observed_at: null,
  last_observed_at: null,
  next_action: "retry verification",
};

describe("snapshot schema", () => {
  it("the fixture parses and terminals is false", () => {
    const snapshot = snapshotSchema.parse(fixtureJson);

    expect(snapshot.status.stages).toHaveLength(7);
    expect(stageStatusSchema.options).toHaveLength(13);
    expect(snapshot.terminals).toBe(false);
  });

  it("a frame without terminals fails to parse", () => {
    const withoutTerminals = structuredClone(fixtureJson) as Record<string, unknown>;
    delete withoutTerminals.terminals;

    expect(snapshotSchema.safeParse(withoutTerminals).success).toBe(false);
  });

  it("rejects an unknown stage status", () => {
    const invalid = structuredClone(fixtureJson) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("parses a stage's cleanup_warning when present, and still parses when it's absent", () => {
    // The fixture's stages never carry the key (Rust's skip_serializing_if
    // omits it when None), so this is the only place the present case is
    // exercised for a *stage* (WebAttention entries pin the null case).
    const withWarning = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    withWarning.status.stages[0].cleanup_warning = "leftover";
    const parsed = snapshotSchema.parse(withWarning);
    expect(parsed.status.stages[0].cleanup_warning).toBe("leftover");

    const withoutKey = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    for (const stage of withoutKey.status.stages) {
      delete stage.cleanup_warning;
    }
    expect(snapshotSchema.safeParse(withoutKey).success).toBe(true);
  });

  it("parses pending and parked completion blockers with an outgoing exit reason", () => {
    const recovery = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    recovery.status.stages[0] = {
      ...recovery.status.stages[0],
      status: "executing",
      completion_blocker: {
        state: "pending",
        fingerprint: "fp-pending",
        failure_code: "boundary-check-failed",
        summary: null,
        commit: "abc123",
        repeat_count: 1,
        first_observed_at: "2026-09-14T08:00:00Z",
        last_observed_at: null,
        next_action: "wait for the next verification pass",
      },
    };
    recovery.status.stages[1] = {
      ...recovery.status.stages[1],
      status: "needs-human-review",
      outgoing_session_exit_reason: "criteria-blocked",
      completion_blocker: {
        state: "blocked",
        fingerprint: "fp-blocked",
        failure_code: "criteria-blocked",
        summary: "Acceptance criteria remain unmet",
        commit: "def456",
        repeat_count: 2,
        first_observed_at: "2026-09-14T08:00:00Z",
        last_observed_at: "2026-09-14T08:05:00Z",
        next_action: "review the completion evidence",
      },
    };

    const parsed = snapshotSchema.parse(recovery);

    expect(parsed.status.stages[0].completion_blocker?.state).toBe("pending");
    expect(parsed.status.stages[1]).toMatchObject({
      status: "needs-human-review",
      outgoing_session_exit_reason: "criteria-blocked",
      completion_blocker: { state: "blocked" },
    });
  });

  it.each<readonly [string, (stage: Record<string, unknown>) => void]>([
    [
      "completion state",
      (stage) => {
        stage.completion_blocker = { ...validBlocker, state: "waiting" };
      },
    ],
    [
      "exit reason",
      (stage) => {
        stage.outgoing_session_exit_reason = "unknown";
      },
    ],
    [
      "extra blocker key",
      (stage) => {
        stage.completion_blocker = { ...validBlocker, extra: true };
      },
    ],
  ])("rejects a malformed %s", (_label, corrupt) => {
    const invalid = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    corrupt(invalid.status.stages[0]);

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("still parses a stage without completion recovery fields", () => {
    const parsed = snapshotSchema.parse(fixtureJson);

    expect(parsed.status.stages[0].completion_blocker).toBeUndefined();
    expect(parsed.status.stages[0].outgoing_session_exit_reason).toBeUndefined();
  });

  it("accepts an omitted healthy-case notice", () => {
    expect(snapshotSchema.parse(fixtureJson).notice).toBeUndefined();
  });

  it("parses the quota block", () => {
    const quota = snapshotSchema.parse(fixtureJson).status.quota;

    expect(quota.claude?.windows.map((window) => window.kind)).toEqual(["five-hour", "seven-day"]);
    expect(quota.codex?.windows).toHaveLength(1);
    expect(quota.codex?.plan).toBe("pro");
  });

  it("rejects a status without quota", () => {
    const withoutQuota = structuredClone(fixtureJson) as {
      status: Record<string, unknown>;
    };
    delete withoutQuota.status.quota;

    expect(snapshotSchema.safeParse(withoutQuota).success).toBe(false);
  });

  it("accepts a provider with no data", () => {
    const withoutCodex = structuredClone(fixtureJson) as {
      status: { quota: { codex: unknown } };
    };
    withoutCodex.status.quota.codex = null;

    expect(snapshotSchema.safeParse(withoutCodex).success).toBe(true);
  });

  it("parses a stage whose session_type is the contract-writer phase", () => {
    const withContract = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    withContract.status.stages[0].status = "executing";
    withContract.status.stages[0].session_type = "contract";

    const parsed = snapshotSchema.safeParse(withContract);

    expect(parsed.success).toBe(true);
    expect(parsed.success && parsed.data.status.stages[0].session_type).toBe("contract");
  });
});
