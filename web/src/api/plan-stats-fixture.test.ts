import { describe, expect, it } from "vitest";

import planStatsJson from "@/api/fixtures/plan-stats-snapshot.json";
import snapshotJson from "@/api/fixtures/snapshot.json";
import { parseFrame } from "@/api/frames";
import { planStatsSchema, snapshotSchema } from "@/api/schema";

// Both fixtures are Rust serde output (`model_tests_plan_stats.rs` rebuilds them and
// fails when they drift), so a strict parse here is the round trip Rust -> browser.
describe("plan overview wire fixtures", () => {
  it("parses the snapshot with phases and a completion time on one stage", () => {
    const client = snapshotSchema.parse(snapshotJson).status.stages.find((s) => s.id === "client");

    expect(client?.completed_at).toBe("2026-09-04T12:00:00Z");
    expect(client?.phases?.map((span) => span.phase)).toEqual(["executing", "retry"]);
    expect(client?.phases?.every((span) => span.ended_at)).toBe(true);
  });

  it("parses a snapshot carrying the plan report, strictly", () => {
    const status = snapshotSchema.parse(planStatsJson).status;

    expect(status.plan_stats?.plan_name).toBe("Web Dashboard Fixture");
    expect(status.plan_stats?.stages.map((stage) => stage.status)).toEqual([
      "completed",
      "skipped",
    ]);
    expect(status.plan_stats?.stages[1]?.started_at).toBeNull();
    expect(
      planStatsSchema.safeParse({ ...planStatsJson.status.plan_stats, extra: 1 }).success,
    ).toBe(false);
  });

  it("carries the report through a full frame", () => {
    const parsed = parseFrame({ t: "full", snapshot: planStatsJson });

    expect(parsed.success).toBe(true);
    if (parsed.success && parsed.data.t === "full") {
      expect(parsed.data.snapshot.status.plan_stats?.models[0]?.lane).toBe("claude");
    }
  });

  it("accepts a state frame for a stage whose phases the feed left out", () => {
    const client = snapshotSchema.parse(snapshotJson).status.stages.find((s) => s.id === "client")!;
    const { phases: _phases, id: _id, name: _name, ...rest } = client;
    const dynamic = Object.fromEntries(
      Object.entries(rest).filter(
        ([key]) =>
          !["description", "summary", "stage_type", "dependencies", "max_retries"].includes(key),
      ),
    );

    const parsed = parseFrame({
      t: "state",
      at: "2026-09-04T12:00:01Z",
      stages: { client: dynamic },
    });

    expect(parsed.success).toBe(true);
  });
});
