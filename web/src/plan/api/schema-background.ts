import { z } from "@/api/zod";

const u64 = z.number().int().min(0);

export const taskKindSchema = z.enum(["agent", "shell"]);
export const taskStateSchema = z.enum([
  "running",
  "tool-wait",
  "stalled",
  "done",
  "failed",
  "stopped",
]);

// One background task of the planner session. Every key is always on the wire; the optional
// ones arrive as null (a shell has no agent or token fields).
export const backgroundTaskViewSchema = z.strictObject({
  id: z.string(),
  kind: taskKindSchema,
  label: z.string().nullable(),
  agent_type: z.string().nullable(),
  tool_use_id: z.string().nullable(),
  state: taskStateSchema,
  started_at: z.string(),
  last_activity_at: z.string(),
  steps: u64.nullable(),
  last_tool: z.string().nullable(),
  report_preview: z.string().nullable(),
  model: z.string().nullable(),
  total: u64.nullable(),
  tokens_per_min: u64.nullable(),
  active: z.boolean().nullable(),
});

export const backgroundMessageSchemas = [
  z.strictObject({
    type: z.literal("background"),
    session: z.string(),
    tasks: z.array(backgroundTaskViewSchema),
  }),
] as const;

export type TaskKind = z.infer<typeof taskKindSchema>;
export type TaskState = z.infer<typeof taskStateSchema>;
export type BackgroundTaskView = z.infer<typeof backgroundTaskViewSchema>;
export type BackgroundFrame = { session: string; tasks: BackgroundTaskView[] };
