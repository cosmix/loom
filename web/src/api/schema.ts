import { z } from "@/api/zod";

export const STAGE_STATUSES = [
  "waiting-for-deps",
  "queued",
  "executing",
  "waiting-for-input",
  "blocked",
  "completed",
  "needs-handoff",
  "skipped",
  "merge-conflict",
  "completed-with-failures",
  "merge-blocked",
  "needs-human-review",
  "needs-adjudication",
] as const;
export const stageStatusSchema = z.enum(STAGE_STATUSES);
export const stageTypeSchema = z.enum([
  "standard",
  "knowledge",
  "integration-verify",
  "knowledge-distill",
]);
export const activityStatusSchema = z.enum([
  "Idle",
  "Working",
  "Error",
  "Stale",
  "Busy",
  "Quiet",
  "Orphaned",
]);
export const failureTypeSchema = z.enum([
  "session-crash",
  "context-exhausted",
  "test-failure",
  "build-failure",
  "code-error",
  "timeout",
  "user-blocked",
  "merge-conflict",
  "infrastructure-error",
  "sandbox-setup-failure",
  "startup-refusal",
  "unknown",
]);
export const sessionTypeSchema = z.enum([
  "stage",
  "merge",
  "baseconflict",
  "knowledge",
  "adjudication",
  "contract",
]);
export const sessionBackendSchema = z.enum(["native", "tmux"]);
export const outgoingSessionExitReasonSchema = z.enum([
  "completed",
  "crashed",
  "context-ceiling",
  "stalled",
  "operator-stop",
  "criteria-blocked",
  "replaced",
]);
export const completionBlockerStateSchema = z.enum(["pending", "blocked", "ownership_unknown"]);
export const completionBlockerSchema = z
  .object({
    state: completionBlockerStateSchema,
    fingerprint: z.string(),
    failure_code: z.string(),
    summary: z.string().nullable(),
    commit: z.string(),
    repeat_count: z.number().int().nonnegative(),
    first_observed_at: z.string().nullable(),
    last_observed_at: z.string().nullable(),
    next_action: z.string(),
  })
  .strict();
export const failureInfoSchema = z.object({
  failure_type: failureTypeSchema,
  detected_at: z.string(),
  evidence: z.array(z.string()),
});
export const failingCriterionSchema = z.object({
  index: z.number().int().nonnegative(),
  command: z.string(),
  diagnostic: z.string().nullable(),
});
export const disputeOpSchema = z.object({
  op: z.string(),
  index: z.number().int().nonnegative().nullable(),
});
export const disputeLineSchema = z.object({
  id: z.number().int(),
  kind: z.string(),
  verdict: z.string().nullable(),
  field: z.string().nullable(),
  ops: z.array(disputeOpSchema),
});
export const blockDetailSchema = z.object({
  failing_criteria: z.array(failingCriterionSchema),
  disputes: z.array(disputeLineSchema),
});
export const stageSummarySchema = z
  .object({
    id: z.string(),
    name: z.string(),
    description: z.string().nullable(),
    summary: z.string().nullable(),
    status: stageStatusSchema,
    stage_type: stageTypeSchema,
    dependencies: z.array(z.string()),
    context_tokens: z.number().int().nullable(),
    elapsed_secs: z.number().int().nullable(),
    execution_secs: z.number().int().nullable(),
    base_branch: z.string().nullable(),
    base_merged_from: z.array(z.string()),
    failure_info: failureInfoSchema.nullable(),
    activity_status: activityStatusSchema,
    last_tool: z.string().nullable(),
    last_activity: z.string().nullable(),
    staleness_secs: z.number().int().nullable(),
    context_ceiling_tokens: z.number().int().nullable(),
    review_reason: z.string().nullable(),
    review_notes: z.string().nullable(),
    merged: z.boolean(),
    merge_assumed: z.boolean(),
    cleanup_warning: z.string().nullable().optional(),
    merge_block: z.string().nullable().optional(),
    stash_warning: z.string().nullable().optional(),
    outgoing_session_exit_reason: outgoingSessionExitReasonSchema.optional(),
    completion_blocker: completionBlockerSchema.optional(),
    held: z.boolean(),
    retry_count: z.number().int(),
    max_retries: z.number().int().nullable(),
    pid: z.number().int().nullable(),
    session_alive: z.boolean(),
    model: z.string(),
    session_type: sessionTypeSchema.nullable(),
    incoherence: z.string().nullable(),
    execution_models: z.array(z.string()),
    dispute_count: z.number().int(),
    judge_heartbeat_secs: z.number().int().nullable(),
    session_backend: sessionBackendSchema.nullable(),
    merge_resolver_session: z.string().optional(),
    merge_resolver_attempts: z.number().int().optional(),
    close_reason: z.string().optional(),
    acceptance: z.array(z.string()).optional(),
    terminal_history_session: z.string().optional(),
    block_detail: blockDetailSchema.optional(),
  })
  .strict();
export const mergeSummarySchema = z.object({
  merged: z.array(z.string()),
  pending: z.array(z.string()),
  conflicts: z.array(z.string()),
});
export const progressSummarySchema = z.object({
  total: z.number().int(),
  completed: z.number().int(),
  executing: z.number().int(),
  pending: z.number().int(),
  blocked: z.number().int(),
});
export const windowKindSchema = z.enum(["five-hour", "seven-day"]);
export const quotaWindowSchema = z.object({
  kind: windowKindSchema,
  used_percent: z.number(),
  resets_at: z.number().int().nullable(),
});
export const providerQuotaSchema = z.object({
  observed_at: z.number().int(),
  windows: z.array(quotaWindowSchema),
  plan: z.string().nullable(),
  error: z.string().nullable(),
});
export const quotaSnapshotSchema = z.object({
  claude: providerQuotaSchema.nullable(),
  codex: providerQuotaSchema.nullable(),
});
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
    model: z.string().nullable(),
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
export const statusDataSchema = z.object({
  stages: z.array(stageSummarySchema),
  merge: mergeSummarySchema,
  progress: progressSummarySchema,
  plan_name: z.string().nullable(),
  quota: quotaSnapshotSchema,
  tokens: tokenFlowSchema.optional(),
});
export const alternativeSchema = z.object({
  when: z.string(),
  command: z.string(),
});
export const attentionSchema = z.object({
  id: z.string(),
  name: z.string(),
  label: z.string(),
  command: z.string().nullable(),
  command_when: z.string().nullable(),
  alternatives: z.array(alternativeSchema),
  note: z.string().nullable(),
  automatic: z.boolean(),
  failure_type: failureTypeSchema.nullable(),
  failure_label: z.string().nullable(),
  evidence: z.array(z.string()),
  review_reason: z.string().nullable(),
  reason: z.string().nullable(),
  acceptance: z.array(z.string()),
  failing_criteria: z.array(failingCriterionSchema),
  disputes: z.array(disputeLineSchema),
  terminal_history: z.boolean(),
  cleanup_warning: z.string().nullable(),
  has_human_review_choices: z.boolean(),
  dispute_count: z.number().int().nullable(),
  judge_heartbeat_secs: z.number().int().nullable(),
});
export const alertSchema = z.object({
  severity: z.enum(["info", "warning", "critical"]),
  text: z.string(),
});
export const daemonStateSchema = z.enum(["running", "process-only", "not-running", "unreachable"]);
export const snapshotSchema = z
  .object({
    status: statusDataSchema,
    attention: z.array(attentionSchema),
    alerts: z.array(alertSchema),
    daemon: daemonStateSchema,
    tick_age_secs: z.number().int().nullable(),
    source: z.enum(["daemon", "files"]),
    terminals: z.boolean(),
    notice: z.string().optional(),
    generated_at: z.string(),
    version: z.string(),
  })
  .strict();

const TOKENS_PATH = ["status", "tokens"] as const;

function isTokensIssue(issue: { path: PropertyKey[] }): boolean {
  return TOKENS_PATH.every((key, index) => issue.path[index] === key);
}

// Only called once every issue sits under `status.tokens`, so zod has already
// walked `raw` and `raw.status` as objects.
function withoutTokens(raw: unknown): unknown {
  const frame = raw as { status: Record<string, unknown> };
  const status = { ...frame.status };
  delete status.tokens;
  return { ...frame, status };
}

// Token flow is an overlay: a frame whose only defects are inside `tokens`
// parses with the block absent, so ordinary status still renders. Any defect
// outside it fails the frame exactly as `snapshotSchema.safeParse` does.
export function parseSnapshot(raw: unknown): ReturnType<typeof snapshotSchema.safeParse> {
  const result = snapshotSchema.safeParse(raw);
  if (result.success || !result.error.issues.every(isTokensIssue)) {
    return result;
  }
  return snapshotSchema.safeParse(withoutTokens(raw));
}

export type StageStatus = z.infer<typeof stageStatusSchema>;
export type StageSummary = z.infer<typeof stageSummarySchema>;
export type StatusData = z.infer<typeof statusDataSchema>;
export type FailingCriterion = z.infer<typeof failingCriterionSchema>;
export type DisputeLine = z.infer<typeof disputeLineSchema>;
export type Attention = z.infer<typeof attentionSchema>;
export type Alternative = z.infer<typeof alternativeSchema>;
export type Alert = z.infer<typeof alertSchema>;
export type DaemonState = z.infer<typeof daemonStateSchema>;
export type FailureType = z.infer<typeof failureTypeSchema>;
export type ActivityStatus = z.infer<typeof activityStatusSchema>;
export type SessionExitReason = z.infer<typeof outgoingSessionExitReasonSchema>;
export type OutgoingSessionExitReason = SessionExitReason;
export type CompletionBlockerState = z.infer<typeof completionBlockerStateSchema>;
export type CompletionBlocker = z.infer<typeof completionBlockerSchema>;
export type Snapshot = z.infer<typeof snapshotSchema>;
export type WindowKind = z.infer<typeof windowKindSchema>;
export type QuotaWindow = z.infer<typeof quotaWindowSchema>;
export type ProviderQuota = z.infer<typeof providerQuotaSchema>;
export type QuotaSnapshot = z.infer<typeof quotaSnapshotSchema>;
export type TokenUsage = z.infer<typeof tokenUsageSchema>;
export type ModelUsage = z.infer<typeof modelUsageSchema>;
export type AgentFlow = z.infer<typeof agentFlowSchema>;
export type StageFlow = z.infer<typeof stageFlowSchema>;
export type PlanFlow = z.infer<typeof planFlowSchema>;
export type QuotaLeft = z.infer<typeof quotaLeftSchema>;
export type TokenFlow = z.infer<typeof tokenFlowSchema>;
