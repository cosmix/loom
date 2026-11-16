import { z } from "@/api/zod";

export const tokenUsageSchema = z
  .object({
    input: z.number().int().nonnegative(),
    cache_write: z.number().int().nonnegative(),
    cache_read: z.number().int().nonnegative(),
    output: z.number().int().nonnegative(),
  })
  .strict();
export const agentFlowSchema = z
  .object({
    id: z.string(),
    kind: z.enum(["main", "subagent", "codex"]),
    agent_type: z.string().nullable(),
    forwarder: z.string().nullable(),
    model: z.string().nullable(),
    // The last reasoning effort the agent's transcript named; null when none did.
    effort: z.string().nullable(),
    // A native codex worker's unit id; null for every other agent.
    unit_id: z.string().nullable(),
    active: z.boolean(),
    total: tokenUsageSchema,
    rate: tokenUsageSchema,
    series: z.array(z.number().int().nonnegative()),
  })
  .strict();
export const modelUsageSchema = z
  .object({
    model: z.string(),
    effort: z.string().nullable(),
    total: tokenUsageSchema,
  })
  .strict();
export const stageFlowSchema = z
  .object({
    stage_id: z.string(),
    total: tokenUsageSchema,
    rate: tokenUsageSchema,
    series: z.array(z.number().int().nonnegative()),
    hit_series: z.array(z.number().int().min(0).max(100).nullable()),
    agents: z.array(agentFlowSchema),
  })
  .strict();
export const planFlowSchema = z
  .object({
    total: tokenUsageSchema,
    rate: tokenUsageSchema,
    series: z.array(z.number().int().nonnegative()),
  })
  .strict();
export const quotaLeftSchema = z
  .object({
    provider: z.enum(["claude", "codex"]),
    minutes_left: z.number().int().nonnegative().nullable(),
    lasts_to_reset: z.boolean(),
  })
  .strict();
export const tokenFlowSchema = z
  .object({
    observed_at: z.number().int(),
    bucket_secs: z.number().int(),
    plan: planFlowSchema,
    stages: z.array(stageFlowSchema),
    models: z.array(modelUsageSchema),
    quota_left: z.array(quotaLeftSchema),
    unreadable_lines: z.number().int().nonnegative(),
  })
  .strict();

export const CODEX_PHASES = ["admitted", "starting", "running", "finalizing", "terminal"] as const;
export const CODEX_OUTCOMES = ["succeeded", "failed", "cancelled", "timed_out", "unknown"] as const;

/// One native codex worker invocation of a stage (`codex_jobs` in the status
/// snapshot). `thread_id` is null when the unit failed before a Codex thread
/// started; `ended_at` is null until the invocation is terminal.
export const codexJobSchema = z
  .object({
    unit_id: z.string(),
    invocation_id: z.string(),
    model: z.string(),
    effort: z.string(),
    phase: z.enum(CODEX_PHASES),
    outcome: z.enum(CODEX_OUTCOMES).nullable(),
    outcome_reason: z.string().nullable(),
    thread_id: z.string().nullable(),
    started_at: z.string(),
    ended_at: z.string().nullable(),
    deadline_secs: z.number().int().nonnegative(),
  })
  .strict();

export type TokenUsage = z.infer<typeof tokenUsageSchema>;
export type ModelUsage = z.infer<typeof modelUsageSchema>;
export type AgentFlow = z.infer<typeof agentFlowSchema>;
export type StageFlow = z.infer<typeof stageFlowSchema>;
export type PlanFlow = z.infer<typeof planFlowSchema>;
export type QuotaLeft = z.infer<typeof quotaLeftSchema>;
export type TokenFlow = z.infer<typeof tokenFlowSchema>;
export type CodexJob = z.infer<typeof codexJobSchema>;
