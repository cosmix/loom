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
});
