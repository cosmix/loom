import { z } from "@/api/zod";

const u32 = z.number().int().min(0).max(4294967295);
const i32 = z.number().int().min(-2147483648).max(2147483647);

export const workspaceStateSchema = z.enum(["absent", "initialized", "running", "stopped", "done"]);
export const runOfferSchema = z.enum(["start", "start_initialized", "none_running", "resume", "start_next"]);
export const backendSchema = z.enum(["tmux", "native"]);
export const runJobSchema = z.enum(["commit", "start", "abandon", "finish_abandon", "dashboard"]);
export const outputStreamSchema = z.enum(["stdout", "stderr"]);

export const planChoiceSchema = z.strictObject({
  path: z.string(),
  slug: z.string(),
  listed: z.boolean(),
});
export const changedPathSchema = z.strictObject({ path: z.string(), status: z.string() });
export const unmergedBranchSchema = z.strictObject({ branch: z.string(), ahead: u32 });
export const abandonViewSchema = z.strictObject({
  plan: z.string(),
  merged_stages: z.array(z.string()),
  unmerged_stages: z.array(z.string()),
  unmerged_branches: z.array(unmergedBranchSchema),
  base_branch: z.string(),
});
export const runStateSchema = z.strictObject({
  workspace: workspaceStateSchema,
  active_plan: z.string().nullable(),
  offer: runOfferSchema,
  plans: z.array(planChoiceSchema),
  preselected: z.string().nullable(),
  branch: z.string(),
  doc_changes: z.array(changedPathSchema),
  blocking_changes: z.array(changedPathSchema),
  backends: z.array(backendSchema),
  default_backend: backendSchema,
  clean_notice: z.boolean(),
  abandon: abandonViewSchema.nullable(),
  finish_abandon: z.string().nullable(),
  dashboard_url: z.string().nullable(),
  busy: runJobSchema.nullable(),
});
export const commitRequestSchema = z.strictObject({ paths: z.array(z.string()), message: z.string() });
export const runStartRequestSchema = z.strictObject({
  plan: z.string(),
  backend: backendSchema,
  clean: z.boolean(),
});
export const abandonRequestSchema = z.strictObject({ confirmed_unmerged: z.array(unmergedBranchSchema) });
export const runOutputSchema = z.strictObject({
  job: runJobSchema,
  step: z.string(),
  stream: outputStreamSchema.nullable(),
  line: z.string().nullable(),
  exit_code: i32.nullable(),
  done: z.boolean(),
});
export const runStripSchema = z.strictObject({
  plan: z.string().nullable(),
  daemon_running: z.boolean(),
  merged: u32,
  total: u32,
  attention: u32,
  dashboard_url: z.string().nullable(),
});
export const runMessageSchemas = [
  z.strictObject({ type: z.literal("run_state"), ...runStateSchema.shape }),
  z.strictObject({ type: z.literal("run_output"), ...runOutputSchema.shape }),
  z.strictObject({ type: z.literal("run_strip"), ...runStripSchema.shape }),
] as const;

export type WorkspaceState = z.infer<typeof workspaceStateSchema>;
export type RunOffer = z.infer<typeof runOfferSchema>;
export type Backend = z.infer<typeof backendSchema>;
export type RunJob = z.infer<typeof runJobSchema>;
export type OutputStream = z.infer<typeof outputStreamSchema>;
export type PlanChoice = z.infer<typeof planChoiceSchema>;
export type ChangedPath = z.infer<typeof changedPathSchema>;
export type UnmergedBranch = z.infer<typeof unmergedBranchSchema>;
export type AbandonView = z.infer<typeof abandonViewSchema>;
export type RunStateView = z.infer<typeof runStateSchema>;
export type CommitRequest = z.infer<typeof commitRequestSchema>;
export type RunStartRequest = z.infer<typeof runStartRequestSchema>;
export type AbandonRequest = z.infer<typeof abandonRequestSchema>;
export type RunOutput = z.infer<typeof runOutputSchema>;
export type RunStrip = z.infer<typeof runStripSchema>;
