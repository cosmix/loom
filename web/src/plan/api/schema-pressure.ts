import { z } from "@/api/zod";

const u32 = z.number().int().min(0).max(4294967295);
const u64 = z.number().int().min(0);

export const pressureStepKindSchema = z.enum(["claude_pressure", "codex", "claude_address"]);
export const stepStateSchema = z.enum(["running", "done", "failed", "stopped"]);
export const codexStateSchema = z.enum(["running", "review_written", "no_review", "failed"]);
export const runEndKindSchema = z.enum(["recorded", "stopped", "failed", "interrupted"]);

export const pressureSlotsSchema = z.strictObject({
  rounds: u32,
  claude_model: z.string(),
  claude_effort: z.string(),
  codex_model: z.string(),
  codex_effort: z.string(),
  address_model: z.string(),
  address_effort: z.string(),
});
export const pressureOptionsSchema = z.strictObject({
  available: z.boolean(),
  disabled_reason: z.string().nullable(),
  plans: z.array(z.string()),
  defaults: pressureSlotsSchema,
  claude_models: z.array(z.string()),
  claude_efforts: z.array(z.string()),
  codex_models: z.array(z.string()),
  codex_efforts: z.array(z.string()),
  running: z.boolean(),
});
export const codexStatusSchema = z.strictObject({
  model: z.string(),
  state: codexStateSchema,
  report_path: z.string().nullable(),
  log_tail: z.string().nullable(),
});
export const tokenTotalsSchema = z.strictObject({
  input: u64,
  cache_write: u64,
  cache_read: u64,
  output: u64,
});
export const pressureStepViewSchema = z.strictObject({
  round: u32,
  kind: pressureStepKindSchema,
  session_id: z.string().nullable(),
  model: z.string(),
  effort: z.string(),
  state: stepStateSchema,
  started_ms: u64,
  ended_ms: u64.nullable(),
  tokens: tokenTotalsSchema.nullable(),
  codex: codexStatusSchema.nullable(),
  edits: z.array(z.string()),
});
export const pressureEndSchema = z.strictObject({
  kind: runEndKindSchema,
  by: z.string().nullable(),
  step: z.string().nullable(),
  reason: z.string().nullable(),
  line: z.string().nullable(),
});
export const pressureRunSchema = z.strictObject({
  plan: z.string(),
  rounds: u32,
  round: u32,
  step: pressureStepKindSchema,
  started_ms: u64,
  tokens_total: u64,
  steps: z.array(pressureStepViewSchema),
  ended: pressureEndSchema.nullable(),
});
export const pressureMessageSchemas = [
  z.strictObject({ type: z.literal("pressure_run"), ...pressureRunSchema.shape }),
] as const;

export type PressureStepKind = z.infer<typeof pressureStepKindSchema>;
export type StepState = z.infer<typeof stepStateSchema>;
export type CodexState = z.infer<typeof codexStateSchema>;
export type RunEndKind = z.infer<typeof runEndKindSchema>;
export type PressureSlots = z.infer<typeof pressureSlotsSchema>;
export type PressureOptions = z.infer<typeof pressureOptionsSchema>;
export type CodexStatus = z.infer<typeof codexStatusSchema>;
export type TokenTotals = z.infer<typeof tokenTotalsSchema>;
export type PressureStepView = z.infer<typeof pressureStepViewSchema>;
export type PressureEnd = z.infer<typeof pressureEndSchema>;
export type PressureRunView = z.infer<typeof pressureRunSchema>;
