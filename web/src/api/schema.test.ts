import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, stageStatusSchema } from "@/api/schema";

describe("snapshot schema", () => {
  it("parses the shared fixture", () => {
    const snapshot = snapshotSchema.parse(fixtureJson);

    expect(snapshot.status.stages).toHaveLength(7);
    expect(stageStatusSchema.options).toHaveLength(13);
  });

  it("rejects an unknown stage status", () => {
    const invalid = structuredClone(fixtureJson) as {
      status: { stages: Array<{ status: string }> };
    };
    invalid.status.stages[0].status = "bogus";

    expect(snapshotSchema.safeParse(invalid).success).toBe(false);
  });

  it("accepts fixtures without cleanup warnings", () => {
    const withoutCleanupWarnings = structuredClone(fixtureJson) as {
      status: { stages: Array<Record<string, unknown>> };
    };
    for (const stage of withoutCleanupWarnings.status.stages) {
      delete stage.cleanup_warning;
    }

    expect(snapshotSchema.safeParse(withoutCleanupWarnings).success).toBe(true);
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
});
