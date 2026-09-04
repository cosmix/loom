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

  it("accepts an omitted healthy-case notice", () => {
    expect(snapshotSchema.parse(fixtureJson).notice).toBeUndefined();
  });
});
