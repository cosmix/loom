import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { parseSnapshot, snapshotSchema, stageStatusSchema } from "@/api/schema";

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

describe("token flow schema", () => {
  it("parses the fixture's plan token rate", () => {
    const snapshot = snapshotSchema.parse(fixtureJson);

    expect(snapshot.status.tokens?.plan.rate.cache_write).toBe(36000);
  });

  it("parses all three agent kinds in the server stage", () => {
    const tokens = snapshotSchema.parse(fixtureJson).status.tokens;
    const server = tokens?.stages.find((stage) => stage.stage_id === "server");

    expect(server?.agents).toHaveLength(3);
    expect(server?.agents.map((agent) => agent.kind)).toEqual(["main", "subagent", "codex"]);
  });

  it("accepts a snapshot without token flow", () => {
    const withoutTokens = structuredClone(fixtureJson) as {
      status: Record<string, unknown>;
    };
    delete withoutTokens.status.tokens;

    expect(snapshotSchema.parse(withoutTokens).status.tokens).toBeUndefined();
  });

  it("rejects an unknown agent key", () => {
    const invalid = structuredClone(fixtureJson);
    Object.assign(invalid.status.tokens.stages[0].agents[0], { unexpected: true });

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("rejects an unknown key on a models entry", () => {
    const invalid = structuredClone(fixtureJson);
    Object.assign(invalid.status.tokens.models[0], { unexpected: true });

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("rejects a cache hit value of 101", () => {
    const invalid = structuredClone(fixtureJson);
    invalid.status.tokens.stages[0].hit_series[0] = 101;

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("parses null cache hit values", () => {
    const valid = structuredClone(fixtureJson);
    (valid.status.tokens.stages[0].hit_series as (number | null)[])[0] = null;

    expect(snapshotSchema.parse(valid).status.tokens?.stages[0].hit_series[0]).toBeNull();
  });

  it("rejects a negative token count", () => {
    const invalid = structuredClone(fixtureJson);
    invalid.status.tokens.plan.rate.cache_write = -1;

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("a tokens block with a counter of 2**53 parses with tokens undefined and everything else intact", () => {
    const oversized = structuredClone(fixtureJson);
    oversized.status.tokens.plan.total.cache_read = 2 ** 53;

    const result = parseSnapshot(oversized);

    expect(result.success).toBe(true);
    expect(result.data?.status.tokens).toBeUndefined();
    expect(result.data?.status.stages).toHaveLength(9);
    expect(result.data?.status.progress).toEqual(fixtureJson.status.progress);
    expect(result.data?.status.quota).toEqual(fixtureJson.status.quota);
    expect(result.data?.attention).toHaveLength(fixtureJson.attention.length);
    expect(result.data?.version).toBe(fixtureJson.version);
  });

  it("parseSnapshot drops an invalid tokens block from a frame that is otherwise valid", () => {
    const invalid = structuredClone(fixtureJson);
    invalid.status.tokens.plan.rate.cache_write = -1;

    const result = parseSnapshot(invalid);

    expect(result.success).toBe(true);
    expect(result.data?.status.tokens).toBeUndefined();
    expect(result.data?.status.stages).toHaveLength(9);
  });

  it("parseSnapshot still fails a frame that is invalid outside tokens", () => {
    const invalid = structuredClone(fixtureJson);
    invalid.status.stages[0].status = "bogus";

    expect(parseSnapshot(invalid).success).toBe(false);
  });

  it("parseSnapshot still fails when tokens and another field are both invalid", () => {
    const invalid = structuredClone(fixtureJson);
    invalid.status.tokens.plan.rate.cache_write = -1;
    invalid.status.stages[0].status = "bogus";

    const result = parseSnapshot(invalid);

    expect(result.success).toBe(false);
    expect(result.error?.issues.some((issue) => issue.path[1] === "stages")).toBe(true);
  });

  it("parseSnapshot leaves a valid frame untouched", () => {
    expect(parseSnapshot(fixtureJson).data).toEqual(snapshotSchema.parse(fixtureJson));
  });
});

describe("snapshot schema", () => {
  it("the fixture parses and terminals is false", () => {
    const snapshot = snapshotSchema.parse(fixtureJson);

    expect(snapshot.status.stages).toHaveLength(9);
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

  it("keeps the blocked-stage fields optional and the stage object strict", () => {
    const stages = (fixtureJson as { status: { stages: Array<Record<string, unknown>> } }).status
      .stages;
    const blocked = stages.find((stage) => stage.id === "local-runtime");
    const plain = stages.find((stage) => stage.id === "client");

    expect(blocked?.acceptance).toHaveLength(5);
    expect(blocked?.terminal_history_session).toBe("session-9c1d4e7a-1788700000");
    expect(plain && "acceptance" in plain).toBe(false);
    expect(plain && "terminal_history_session" in plain).toBe(false);

    const withUnknown = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    withUnknown.status.stages[0].unexpected = true;
    expect(snapshotSchema.safeParse(withUnknown).success).toBe(false);
  });

  it("requires every attention entry to carry its introduction and alternatives", () => {
    const withoutAlternatives = structuredClone(fixtureJson) as {
      attention: Array<Record<string, unknown>>;
    };
    delete withoutAlternatives.attention[0].alternatives;

    expect(snapshotSchema.safeParse(withoutAlternatives).success).toBe(false);
  });

  it("requires every attention entry to carry the block facts", () => {
    for (const field of [
      "reason",
      "acceptance",
      "failing_criteria",
      "disputes",
      "terminal_history",
    ]) {
      const without = structuredClone(fixtureJson) as {
        attention: Array<Record<string, unknown>>;
      };
      delete without.attention[0][field];

      expect(snapshotSchema.safeParse(without).success, field).toBe(false);
    }
  });

  it("accepts a stage summary without block detail and rejects a malformed failing criterion", () => {
    const stages = (json: unknown) =>
      (json as { status: { stages: Array<Record<string, unknown>> } }).status.stages;
    const plain = structuredClone(fixtureJson);
    delete stages(plain)[0].block_detail;
    expect(snapshotSchema.safeParse(plain).success).toBe(true);

    const malformed = structuredClone(fixtureJson);
    stages(malformed)[0].block_detail = {
      failing_criteria: [{ index: -1, command: "x", diagnostic: null }],
      disputes: [],
    };
    expect(snapshotSchema.safeParse(malformed).success).toBe(false);
  });
});
