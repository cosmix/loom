import { quotaSnapshotSchema } from "@/api/schema";
import { z } from "@/api/zod";

const uint = z.number().int().nonnegative();

export const tokenVectorSchema = z.strictObject({
  fresh_input: uint.optional(),
  cache_write: uint.optional(),
  cache_read: uint.optional(),
  output: uint.optional(),
});

export const costLineSchema = z.strictObject({
  time: z.string(),
  trigger: z.enum(["refinement", "background", "button"]),
  summaries: z.array(z.string()),
  level: z.enum(["document", "overview"]),
  model: z.string().optional(),
  source_bytes: uint.optional(),
  tokens: tokenVectorSchema,
});

export const runningRebuildSchema = z.strictObject({
  session: z.string(),
  ids: z.array(z.string()),
  done: uint,
  total: uint,
  started_at: z.string(),
});

export const failedSummarySchema = z.strictObject({ id: z.string(), error: z.string() });

export const summaryQuestionSchema = z.strictObject({
  id: z.string(),
  count: uint,
  ids: z.array(z.string()),
  estimate_tokens: uint,
  basis: z.enum(["last", "per_byte", "starting"]),
});

export const turnCostRowSchema = z.strictObject({
  id: z.string(),
  tokens: uint.optional(),
});

export const summaryTurnCostSchema = z.strictObject({
  session: z.string(),
  turn: z.string(),
  rows: z.array(turnCostRowSchema),
});

export const summaryPanelSchema = z.strictObject({
  running: runningRebuildSchema.nullable(),
  queue: z.array(z.string()),
  failed: z.array(failedSummarySchema),
  question: summaryQuestionSchema.nullable(),
  last: costLineSchema.nullable(),
  today: tokenVectorSchema,
  week: tokenVectorSchema,
  quota: quotaSnapshotSchema,
  read_only: z.string().nullable(),
  terminal_claims: z.array(z.string()),
  start_error: z.string().nullable(),
});

export const summaryPanelMessageSchema = summaryPanelSchema.extend({
  type: z.literal("summary_panel"),
});
export const summaryQuestionMessageSchema = summaryQuestionSchema.extend({
  type: z.literal("summary_question"),
});
export const summaryQuestionClosedMessageSchema = z.strictObject({
  type: z.literal("summary_question_closed"),
  id: z.string(),
});
export const summaryTurnCostMessageSchema = summaryTurnCostSchema.extend({
  type: z.literal("summary_turn_cost"),
});
export const summaryMessageSchemas = [
  summaryPanelMessageSchema,
  summaryQuestionMessageSchema,
  summaryQuestionClosedMessageSchema,
  summaryTurnCostMessageSchema,
] as const;

export type TokenVector = z.infer<typeof tokenVectorSchema>;
export type SummaryQuestion = z.infer<typeof summaryQuestionSchema>;
export type SummaryPanel = z.infer<typeof summaryPanelSchema>;
