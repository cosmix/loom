import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { planStatsSchema, snapshotSchema } from "@/api/schema";

import { sampleRecordJson } from "./sample-record";

const withRecord = (record: unknown) => ({
  ...fixtureJson,
  status: { ...fixtureJson.status, plan_stats: record },
});

describe("plan stats schema", () => {
  it("parses a populated record, nulls and open spans included", () => {
    const record = snapshotSchema.parse(withRecord(sampleRecordJson)).status.plan_stats;

    expect(record?.stages.map((stage) => stage.status)).toEqual([
      "completed",
      "skipped",
      "completed",
    ]);
    expect(record?.stages[1]?.started_at).toBeNull();
    expect(record?.stages[2]?.phases.at(-1)?.ended_at).toBeNull();
    expect(record?.models[0]?.lane).toBe("codex");
  });

  it("tolerates absent nullable fields and a status without plan_stats", () => {
    const sparse = structuredClone(sampleRecordJson) as Record<string, unknown>;
    const stage = { ...(sparse.stages as Array<Record<string, unknown>>)[1] };
    delete stage.started_at;
    delete stage.model;
    sparse.stages = [stage];

    expect(planStatsSchema.safeParse(sparse).success).toBe(true);
    expect(snapshotSchema.parse(fixtureJson).status.plan_stats).toBeUndefined();
  });

  it("pins schema_version to 1 and rejects unknown keys", () => {
    expect(planStatsSchema.safeParse({ ...sampleRecordJson, schema_version: 2 }).success).toBe(
      false,
    );
    expect(planStatsSchema.safeParse({ ...sampleRecordJson, extra: 1 }).success).toBe(false);
    const phase = { phase: "waiting", started_at: "2026-11-21T09:00:00Z", extra: 1 };
    const stage = { ...sampleRecordJson.stages[0], phases: [phase] };
    expect(planStatsSchema.safeParse({ ...sampleRecordJson, stages: [stage] }).success).toBe(false);
    const model = { ...sampleRecordJson.models[0], lane: "other" };
    expect(planStatsSchema.safeParse({ ...sampleRecordJson, models: [model] }).success).toBe(false);
  });

  it("reads phases and the completion time on a stage summary, and their absence", () => {
    const withSpans = structuredClone(fixtureJson);
    Object.assign(withSpans.status.stages[0], {
      completed_at: null,
      phases: [{ phase: "contract", started_at: "2026-11-21T09:00:00Z", ended_at: null }],
    });
    const parsed = snapshotSchema.parse(withSpans).status.stages[0];

    expect(parsed?.phases?.[0]?.phase).toBe("contract");
    expect(parsed?.completed_at).toBeNull();
    expect(snapshotSchema.parse(fixtureJson).status.stages[0]?.phases).toBeUndefined();
  });
});
