import { planStatsSchema } from "@/api/schema";

const usage = (input: number, cacheWrite: number, cacheRead: number, output: number) => ({
  input,
  cache_write: cacheWrite,
  cache_read: cacheRead,
  output,
});

/// A record as the Rust serializer writes it: every nullable field present as an explicit null.
/// One hour of wall time (09:00Z to 10:00Z); the `docs` stage was skipped.
export const sampleRecordJson = {
  schema_version: 1,
  plan_id: "20261121T090000Z-sample-plan-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10",
  plan_name: "Sample plan",
  started_at: "2026-11-21T09:00:00Z",
  ended_at: "2026-11-21T10:00:00Z",
  wall_secs: 3600,
  execution_secs: 5400,
  totals: usage(1200, 3400, 56000, 780),
  stages: [
    {
      id: "server",
      name: "Rust server",
      stage_type: "standard",
      status: "completed",
      started_at: "2026-11-21T09:00:00Z",
      completed_at: "2026-11-21T09:40:00Z",
      execution_secs: 2400,
      model: "opus",
      effort: "high",
      phases: [
        { phase: "contract", started_at: "2026-11-21T09:00:00Z", ended_at: "2026-11-21T09:10:00Z" },
        {
          phase: "executing",
          started_at: "2026-11-21T09:10:00Z",
          ended_at: "2026-11-21T09:40:00Z",
        },
      ],
      tokens: usage(1000, 3000, 50000, 700),
    },
    {
      id: "docs",
      name: "Documentation",
      stage_type: "standard",
      status: "skipped",
      started_at: null,
      completed_at: null,
      execution_secs: 0,
      model: null,
      effort: null,
      phases: [],
      tokens: usage(0, 0, 0, 0),
    },
    {
      id: "web",
      name: "Web client",
      stage_type: "standard",
      status: "completed",
      started_at: "2026-11-21T09:30:00Z",
      completed_at: "2026-11-21T10:00:00Z",
      execution_secs: 3000,
      model: "gpt-5.6-terra",
      effort: null,
      phases: [
        {
          phase: "executing",
          started_at: "2026-11-21T09:30:00Z",
          ended_at: "2026-11-21T09:45:00Z",
        },
        {
          phase: "adjudication",
          started_at: "2026-11-21T09:45:00Z",
          ended_at: "2026-11-21T09:50:00Z",
        },
        { phase: "retry", started_at: "2026-11-21T09:50:00Z", ended_at: null },
      ],
      tokens: usage(200, 400, 6000, 80),
    },
  ],
  models: [
    {
      model: "gpt-5.6-terra",
      effort: null,
      lane: "codex",
      agents: 1,
      tokens: usage(200, 400, 6000, 80),
    },
    {
      model: "opus",
      effort: "high",
      lane: "claude",
      agents: 2,
      tokens: usage(1000, 3000, 50000, 700),
    },
  ],
};

export const sampleRecord = planStatsSchema.parse(sampleRecordJson);
