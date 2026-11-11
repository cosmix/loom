import { z } from "@/api/zod";
import { kickoffSizeSchema } from "./schema-kickoff";

const u32 = z.number().int().min(0).max(4294967295);
const u64 = z.number().int().min(0);

export const sessionStateSchema = z.enum([
  "starting",
  "idle",
  "working",
  "permission",
  "ending",
  "ended",
  "failed",
]);
export const failureKindSchema = z.enum(["spawn", "died"]);
export const deliverySchema = z.enum(["sent", "delivered", "undelivered"]);
export const originSchema = z.enum(["offer", "closing"]);
export const toolStatusSchema = z.enum(["pending", "done", "error", "denied", "not_finished"]);
export const permissionStatusSchema = z.enum(["pending", "allowed", "denied"]);
export const denyReasonSchema = z.enum([
  "holder",
  "timeout",
  "session_ended",
  "server_stopping",
  "hook_error",
  "interrupt",
  "summary_session",
  "settings",
]);
// The standing rule that answered a WebFetch request without a card.
export const fetchRuleSchema = z.enum(["never", "always", "listed", "session"]);
export const noticeKindSchema = z.enum([
  "interrupted",
  "compacted",
  "api_error",
  "stopped",
  "info",
]);
export const attachmentKindSchema = z.enum(["source", "summary"]);
// A delivered chip also has the `file` kind: an upload, named by id and never by path.
export const chipKindSchema = z.enum(["source", "summary", "file"]);
export const lockedTagSchema = z.enum(["running", "done", "retired", "knowledge", "brief"]);

// Paths and rests_on addresses retain their repo-relative wire form (doc/...).
const attachmentFields = {
  path: z.string(),
  anchor: z.string().nullable(),
  lines: z.tuple([u32, u32]).nullable(),
  stage: z.string().nullable(),
  summary_sentence: z.string().nullable(),
  rests_on: z.array(z.string()),
  quote: z.string(),
};
export const fileMetaSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  media_type: z.string(),
  bytes: u64,
});
// A file chip has `path: ""` and `file` set; the other kinds omit `file`.
export const attachmentSchema = z.strictObject({
  kind: chipKindSchema,
  ...attachmentFields,
  changed_since_selected: z.boolean(),
  locked: lockedTagSchema.nullable(),
  file: fileMetaSchema.optional(),
});
export const attachmentInputSchema = z.strictObject({
  kind: attachmentKindSchema,
  ...attachmentFields,
  selected_hash: z.string().nullable(),
});
// An upload travels by the id the server issued; the client never sends a path.
export const fileAttachmentInputSchema = z.strictObject({
  kind: z.literal("file"),
  id: z.string(),
  name: z.string(),
});
export const anyAttachmentInputSchema = z.union([fileAttachmentInputSchema, attachmentInputSchema]);

export const failureSchema = z.strictObject({
  kind: failureKindSchema,
  message: z.string(),
});
export const sessionViewSchema = z.strictObject({
  id: z.string(),
  kind: z.string(),
  state: sessionStateSchema,
  name: z.string(),
  model: z.string(),
  socket: z.string(),
  holder: z.string().nullable(),
  away_until: z.string().nullable(),
  queued: u32,
  started_at: z.string(),
  state_since: z.string(),
  working_since: z.string().nullable(),
  last_activity: z.string().nullable(),
  failure: failureSchema.nullable(),
  offer: z.string().nullable(),
  feature: z.string().nullable(),
  plan: z.string().nullable(),
  // The holder allowed this session's later web fetches from a card.
  fetch_grant: z.boolean(),
});
export const tokenCountsSchema = z.strictObject({
  input: u64,
  cache_write: u64,
  cache_read: u64,
  output: u64,
});
export const changedFileSchema = z.strictObject({
  path: z.string(),
  before_hash: z.string().nullable(),
  by_tool: z.boolean(),
});

// A WebFetch request's URL as the fetch reads it (host in punycode, cut with a trailing "…").
// `host` is a name, never an IP address; `listable` says it can go on the allowlist;
// `matched` is the allowlist entry that allowed the request (rule "listed").
export const fetchViewSchema = z.strictObject({
  url: z.string(),
  host: z.string().nullable(),
  listable: z.boolean(),
  matched: z.string().nullable(),
  rule: fetchRuleSchema.nullable(),
});

export const itemSchema = z.discriminatedUnion("type", [
  z.strictObject({
    type: z.literal("user"),
    id: z.string(),
    text: z.string(),
    attachments: z.array(attachmentSchema),
    delivery: deliverySchema,
    origin: originSchema.nullable(),
  }),
  z.strictObject({
    type: z.literal("thinking"),
    id: z.string(),
    duration_ms: u64.nullable(),
  }),
  z.strictObject({
    type: z.literal("text"),
    id: z.string(),
    markdown: z.string(),
  }),
  z.strictObject({
    type: z.literal("tool"),
    id: z.string(),
    tool_use_id: z.string(),
    name: z.string(),
    summary: z.string(),
    status: toolStatusSchema,
    subagent: z.string().nullable(),
    parent: z.string().nullable(),
    input: z.string(),
    result: z.string().nullable(),
    result_truncated: z.boolean(),
    error_line: z.string().nullable(),
    added: u32.nullable(),
    removed: u32.nullable(),
    patch: z.string().nullable(),
    nested_tools: u32,
  }),
  z.strictObject({
    type: z.literal("permission"),
    id: z.string(),
    request_id: z.string(),
    tool: z.string(),
    input: z.string(),
    status: permissionStatusSchema,
    deny_reason: denyReasonSchema.nullable(),
    note: z.string().nullable(),
    expires_at: z.string(),
    fetch: fetchViewSchema.nullable(),
  }),
  z.strictObject({
    type: z.literal("notice"),
    id: z.string(),
    kind: noticeKindSchema,
    text: z.string(),
  }),
]);

export const turnSchema = z.strictObject({
  id: z.string(),
  started_at: z.string(),
  duration_ms: u64.nullable(),
  open: z.boolean(),
  tokens: tokenCountsSchema,
  items: z.array(itemSchema),
  changed_files: z.array(changedFileSchema),
  summaries_rebuilding: z.array(z.string()),
});
export const queuedMessageSchema = z.strictObject({
  id: z.string(),
  first_line: z.string(),
  text: z.string().nullable(),
});
export const endedChatSchema = z.strictObject({
  session: sessionViewSchema,
  turns: z.array(turnSchema),
});
export const chatSnapshotSchema = z.strictObject({
  session: sessionViewSchema.nullable(),
  turns: z.array(turnSchema),
  queue: z.array(queuedMessageSchema),
  previous: endedChatSchema.nullable(),
});

export const chatDeltaSchema = z.discriminatedUnion("op", [
  z.strictObject({ op: z.literal("session"), session: sessionViewSchema }),
  z.strictObject({ op: z.literal("turn_added"), turn: turnSchema }),
  z.strictObject({
    op: z.literal("turn_updated"),
    turn_id: z.string(),
    duration_ms: u64.nullable(),
    open: z.boolean(),
    tokens: tokenCountsSchema,
    changed_files: z.array(changedFileSchema),
    summaries_rebuilding: z.array(z.string()),
  }),
  z.strictObject({ op: z.literal("item_added"), turn_id: z.string(), item: itemSchema }),
  z.strictObject({ op: z.literal("item_replaced"), turn_id: z.string(), item: itemSchema }),
  z.strictObject({ op: z.literal("item_removed"), turn_id: z.string(), id: z.string() }),
  z.strictObject({ op: z.literal("queue"), queue: z.array(queuedMessageSchema) }),
  z.strictObject({ op: z.literal("cleared") }),
]);
export const commandResultSchema = z.strictObject({
  client_id: z.string().nullable(),
  ok: z.boolean(),
  error: z.string().nullable(),
});

// Rust's tagged newtype variants flatten the payload beside the frame tag.
const [firstDelta, ...otherDeltas] = chatDeltaSchema.options;
const deltaFrameSchema = z.discriminatedUnion("op", [
  firstDelta.extend({ frame: z.literal("delta") }),
  ...otherDeltas.map((schema) => schema.extend({ frame: z.literal("delta") })),
]);
const snapshotFrameSchema = chatSnapshotSchema.extend({ frame: z.literal("snapshot") });
const resultFrameSchema = commandResultSchema.extend({ frame: z.literal("result") });
const queueReturnedFrameSchema = z.strictObject({
  frame: z.literal("queue_returned"),
  texts: z.array(z.string()),
});
export const chatFrameSchema = z.discriminatedUnion("frame", [
  snapshotFrameSchema,
  deltaFrameSchema,
  resultFrameSchema,
  queueReturnedFrameSchema,
]);

/**
 * The frame as it arrives on the socket: PlanMessage::Chat flattens the frame beside `type`. A
 * discriminated union on `type` cannot hold four members that share one tag value, so the shared
 * planMessageSchema takes this as a plain union beside it.
 */
const chatWire = { type: z.literal("chat") };
export const chatMessageSchema = z.union([
  snapshotFrameSchema.extend(chatWire),
  resultFrameSchema.extend(chatWire),
  queueReturnedFrameSchema.extend(chatWire),
  ...deltaFrameSchema.options.map((delta) => delta.extend(chatWire)),
]);

const clientId = { client_id: z.string().nullable() };
export const chatCommandSchema = z.discriminatedUnion("command", [
  z.strictObject({
    command: z.literal("start"),
    ...clientId,
    offer: z.string().nullable(),
    feature: z.string().nullable().optional(),
    plan: z.string().nullable().optional(),
    text: z.string().nullable(),
    attachments: z.array(anyAttachmentInputSchema),
  }),
  z.strictObject({
    command: z.literal("send"),
    ...clientId,
    text: z.string(),
    attachments: z.array(anyAttachmentInputSchema),
  }),
  z.strictObject({
    command: z.literal("confirm_kickoff"),
    ...clientId,
    size: kickoffSizeSchema,
    name: z.string(),
  }),
  z.strictObject({ command: z.literal("set_aside_kickoff"), ...clientId, session: z.string() }),
  z.strictObject({ command: z.literal("continue_kickoff"), ...clientId, session: z.string() }),
  z.strictObject({ command: z.literal("discard_kickoff"), ...clientId, session: z.string() }),
  z.strictObject({ command: z.literal("withdraw"), ...clientId, queue_id: z.string() }),
  z.strictObject({
    command: z.literal("permission"),
    ...clientId,
    request_id: z.string(),
    allow: z.boolean(),
    note: z.string().nullable(),
  }),
  z.strictObject({
    command: z.literal("allow_session_fetches"),
    ...clientId,
    request_id: z.string(),
  }),
  z.strictObject({ command: z.literal("allow_fetch_host"), ...clientId, request_id: z.string() }),
  z.strictObject({ command: z.literal("ask_fetches_again"), ...clientId }),
  z.strictObject({ command: z.literal("interrupt"), ...clientId }),
  z.strictObject({ command: z.literal("end"), ...clientId }),
]);
export const chatWireCommandSchema = z.union(
  chatCommandSchema.options.map((command) => command.extend(chatWire)),
);

export type SessionState = z.infer<typeof sessionStateSchema>;
export type FailureKind = z.infer<typeof failureKindSchema>;
export type Delivery = z.infer<typeof deliverySchema>;
export type Origin = z.infer<typeof originSchema>;
export type ToolStatus = z.infer<typeof toolStatusSchema>;
export type PermissionStatus = z.infer<typeof permissionStatusSchema>;
export type DenyReason = z.infer<typeof denyReasonSchema>;
export type FetchRule = z.infer<typeof fetchRuleSchema>;
export type FetchView = z.infer<typeof fetchViewSchema>;
export type NoticeKind = z.infer<typeof noticeKindSchema>;
export type AttachmentKind = z.infer<typeof attachmentKindSchema>;
export type LockedTag = z.infer<typeof lockedTagSchema>;
export type Attachment = z.infer<typeof attachmentSchema>;
export type FileMeta = z.infer<typeof fileMetaSchema>;
export type AttachmentInput = z.infer<typeof attachmentInputSchema>;
export type FileAttachmentInput = z.infer<typeof fileAttachmentInputSchema>;
export type AnyAttachmentInput = z.infer<typeof anyAttachmentInputSchema>;
export type Failure = z.infer<typeof failureSchema>;
export type SessionView = z.infer<typeof sessionViewSchema>;
export type TokenCounts = z.infer<typeof tokenCountsSchema>;
export type ChangedFile = z.infer<typeof changedFileSchema>;
export type Item = z.infer<typeof itemSchema>;
export type Turn = z.infer<typeof turnSchema>;
export type QueuedMessage = z.infer<typeof queuedMessageSchema>;
export type EndedChat = z.infer<typeof endedChatSchema>;
export type ChatSnapshot = z.infer<typeof chatSnapshotSchema>;
export type ChatDelta = z.infer<typeof chatDeltaSchema>;
export type CommandResult = z.infer<typeof commandResultSchema>;
export type ChatFrame = z.infer<typeof chatFrameSchema>;
export type ChatCommand = z.infer<typeof chatCommandSchema>;
