import { z } from "@/api/zod";
import {
  parseSnapshot,
  snapshotSchema,
  stageSummarySchema,
  statusDataSchema,
  tokenFlowSchema,
  type Snapshot,
  type TokenFlow,
} from "@/api/schema";

// The `/ws` delta protocol (loom/src/commands/status/web/feed.rs). Every message is
// a JSON object tagged `t`: `full` carries a whole snapshot, the rest carry only
// what moved since the previous message. The key split between them lives in
// loom/src/commands/status/web/feed_split.rs.

// Must list exactly IMMUTABLE_STAGE_KEYS in loom/src/commands/status/web/feed_split.rs
// (schema_parity.rs checks each "<key>: true,").
export const STAGE_IMMUTABLE_MASK = {
  id: true,
  name: true,
  description: true,
  summary: true,
  stage_type: true,
  dependencies: true,
  max_retries: true,
} as const;

export const IMMUTABLE_STAGE_KEYS = Object.keys(
  STAGE_IMMUTABLE_MASK,
) as (keyof typeof STAGE_IMMUTABLE_MASK)[];

// Everything in a stage that a `state` message may replace. Strict, so a key the
// server starts sending without this file knowing about it is a parse error
// rather than a silently dropped field.
export const stageDynamicSchema = stageSummarySchema.omit(STAGE_IMMUTABLE_MASK);

const clockSchema = z
  .object({
    elapsed: z.number().int().optional(),
    exec: z.number().int().optional(),
    stale: z.number().int().optional(),
    judge: z.number().int().optional(),
    contract: z.number().int().optional(),
  })
  .strict();

const fullFrame = z.object({ t: z.literal("full"), snapshot: z.unknown() }).strict();

const tickFrame = z
  .object({
    t: z.literal("tick"),
    at: z.string(),
    tick_age: z.number().int().nullable(),
    stages: z.record(z.string(), clockSchema),
  })
  .strict();

const stateFrame = z
  .object({
    t: z.literal("state"),
    at: z.string(),
    stages: z.record(z.string(), stageDynamicSchema).optional(),
    status: statusDataSchema
      .pick({ merge: true, progress: true, plan_name: true, quota: true, activity: true })
      .partial()
      .strict()
      .optional(),
    attention: snapshotSchema.shape.attention.optional(),
    alerts: snapshotSchema.shape.alerts.optional(),
    daemon: snapshotSchema.shape.daemon.optional(),
    source: snapshotSchema.shape.source.optional(),
    // null clears the notice; absent leaves it alone.
    notice: z.string().nullable().optional(),
  })
  .strict();

const tokensFrame = z.object({ t: z.literal("tokens"), tokens: z.unknown() }).strict();

const envelope = z.discriminatedUnion("t", [fullFrame, tickFrame, stateFrame, tokensFrame]);

export type TickFrame = z.infer<typeof tickFrame>;
export type StateFrame = z.infer<typeof stateFrame>;
export type Frame =
  | { t: "full"; snapshot: Snapshot }
  | TickFrame
  | StateFrame
  | { t: "tokens"; tokens: TokenFlow | undefined };

export type FrameIssue = { path: PropertyKey[]; message: string };
export type ParsedFrame = { success: true; data: Frame } | { success: false; issues: FrameIssue[] };

/** Validate one `/ws` message. */
export function parseFrame(raw: unknown): ParsedFrame {
  const head = envelope.safeParse(raw);
  if (!head.success) return { success: false, issues: head.error.issues };
  const frame = head.data;
  if (frame.t === "full") {
    const snapshot = parseSnapshot(frame.snapshot);
    if (snapshot.success) return { success: true, data: { t: "full", snapshot: snapshot.data } };
    return {
      success: false,
      issues: snapshot.error.issues.map((issue) => ({
        path: ["snapshot", ...issue.path],
        message: issue.message,
      })),
    };
  }
  if (frame.t === "tokens") {
    return { success: true, data: { t: "tokens", tokens: parseTokens(frame.tokens) } };
  }
  return { success: true, data: frame };
}

// Token flow is an overlay (see parseSnapshot): a defect in it drops the block
// and leaves the rest of the dashboard running.
function parseTokens(raw: unknown): TokenFlow | undefined {
  const result = tokenFlowSchema.nullable().safeParse(raw);
  if (result.success) return result.data ?? undefined;
  console.error("dashboard: bad tokens frame", result.error.issues);
  return undefined;
}
