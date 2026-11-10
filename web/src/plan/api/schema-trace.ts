import { z } from "@/api/zod";

import { saveConflictSchema, saveInvalidSchema, validationViewSchema } from "./schema-files";

// Mirrors `loom/src/plan_server/wire/trace.rs`. Paths are repository-relative with `/`
// separators. Finding text, titles and reasons are agent-written: render them as text only.

export const DOC_ROLES = ["vision", "section", "metaplan", "plan"] as const;
export const PROGRESS_KINDS = ["stub", "written", "planned", "built", "running", "done"] as const;
export const SETTLE_STATES = [
  "draft",
  "revised",
  "stub_changed",
  "upstream_changed",
  "settled",
] as const;
export const COVER_KINDS = ["entry", "plan", "stage"] as const;
export const GAP_KINDS = [
  "revised",
  "stub_changed",
  "upstream_changed",
  "revisit",
  "settle",
  "uncovered",
  "triage",
] as const;
export const FINDING_KINDS = ["open", "planned", "folded", "dismissed"] as const;
export const SETTLE_COMMIT_STATES = ["committed", "unchanged", "not_committed"] as const;

const u32 = z.number().int().min(0).max(4294967295);

export const docRoleSchema = z.enum(DOC_ROLES);
export const progressKindSchema = z.enum(PROGRESS_KINDS);
export const settleStateSchema = z.enum(SETTLE_STATES);
export const coverKindSchema = z.enum(COVER_KINDS);
export const gapKindSchema = z.enum(GAP_KINDS);
export const findingKindSchema = z.enum(FINDING_KINDS);

export const nodeRefSchema = z.strictObject({ path: z.string(), anchor: z.string().nullable() });
export const settledLineSchema = z.strictObject({
  body: z.string(),
  stub: z.string().nullable(),
  upstream: z.string().nullable(),
});
export const traceDocSchema = z.strictObject({
  path: z.string(),
  role: docRoleSchema,
  title: z.string(),
  progress: progressKindSchema,
  settle: settleStateSchema,
  /** Whether the reader is asked to settle it: done work (a locked plan, a done feature's
   *  vision and metaplan) and stubs are not, and show no settle state anywhere. */
  settleable: z.boolean(),
  stub: nodeRefSchema.nullable(),
  number: u32.nullable(),
  slug: z.string().nullable(),
  bundle: z.array(z.string()),
  my_calls: u32,
  open_questions: u32,
  current: settledLineSchema,
});
export const coverRefSchema = z.strictObject({
  kind: coverKindSchema,
  path: z.string(),
  anchor: z.string().nullable(),
  slug: z.string().nullable(),
  stage: z.string().nullable(),
});
export const traceHeadingSchema = z.strictObject({
  path: z.string(),
  anchor: z.string(),
  title: z.string(),
  covered_by: z.array(coverRefSchema),
  findings: z.array(z.string()),
});
export const traceEntrySchema = z.strictObject({
  anchor: z.string(),
  title: z.string(),
  slug: z.string().nullable(),
  plan: z.string().nullable(),
  covers: z.array(nodeRefSchema),
  revisit: z.array(z.string()),
});
export const findingStatusSchema = z.strictObject({
  kind: findingKindSchema,
  detail: z.string().nullable(),
});
export const findingSchema = z.strictObject({
  id: z.string(),
  title: z.string(),
  about: z.array(z.string()),
  source: z.string().nullable(),
  status: findingStatusSchema,
  text: z.string(),
});
export const traceFindingSchema = z.strictObject({
  finding: findingSchema,
  about: z.array(nodeRefSchema),
});
export const gapSchema = z.strictObject({
  kind: gapKindSchema,
  node: nodeRefSchema,
  count: u32.nullable(),
});
export const traceProblemSchema = z.strictObject({
  code: z.string(),
  path: z.string(),
  line: u32.nullable(),
  message: z.string(),
});
export const traceViewSchema = z.strictObject({
  documents: z.array(traceDocSchema),
  headings: z.array(traceHeadingSchema),
  entries: z.array(traceEntrySchema),
  findings: z.array(traceFindingSchema),
  gaps: z.array(gapSchema),
  frontier: z.array(gapSchema),
  problems: z.array(traceProblemSchema),
});

/** `GET /api/settled-version?path=`: the commit holding the document's current Settled line. */
export const settledVersionSchema = z.strictObject({ commit: z.string().nullable() });
/**
 * Where a written line's commit stands when `POST /api/settle` answers: queued as `job` (an id led
 * by the server's own token, so no other server's job carries it), whose end arrives as a
 * `settle_commit` message, or not run at all, with why.
 */
export const settleCommitStartSchema = z.discriminatedUnion("state", [
  z.strictObject({ state: z.literal("pending"), job: z.string() }),
  z.strictObject({ state: z.literal("not_committed"), reason: z.string() }),
]);
/**
 * `POST /api/settle` once the line is written or cleared: the save's answer and where its commit
 * stands. The commit runs after the answer.
 */
export const settleSavedSchema = z.strictObject({
  result: z.literal("saved"),
  hash: z.string(),
  validation: validationViewSchema.nullable(),
  commit: settleCommitStartSchema,
});
/** What a settle that reached the write answers: written, or nothing written (as a save). */
export const settleResultSchema = z.discriminatedUnion("result", [
  settleSavedSchema,
  saveConflictSchema,
  saveInvalidSchema,
]);
/** The 409 `unresolved` answer of `POST /api/settle` without `confirm=1`. */
export const unresolvedSchema = z.strictObject({
  error: z.literal("unresolved"),
  message: z.string(),
  my_calls: u32,
  open_questions: u32,
});

/**
 * How a queued settle commit ended (`settle_commit`, sent to every browser). `commit` is the
 * settle's own commit, set for `committed` (null there when HEAD no longer shows it); `reason` says
 * why a line is `not_committed`, or, in sentences, what is left after a commit (a commit hook that
 * changed the document after it was settled), and is drawn as text. The settling browser reads
 * git's and the hooks' words; every other browser a short reason in their place.
 */
export const settleCommitOutcomeSchema = z.strictObject({
  job: z.string(),
  path: z.string(),
  state: z.enum(SETTLE_COMMIT_STATES),
  commit: z.string().nullable(),
  reason: z.string().nullable(),
});

export const traceMessageSchemas = [
  z.strictObject({ type: z.literal("trace_changed"), feature: z.string() }),
  z.strictObject({ type: z.literal("settle_commit"), ...settleCommitOutcomeSchema.shape }),
] as const;

export type DocRole = z.infer<typeof docRoleSchema>;
export type ProgressKind = z.infer<typeof progressKindSchema>;
export type SettleState = z.infer<typeof settleStateSchema>;
export type CoverKind = z.infer<typeof coverKindSchema>;
export type GapKind = z.infer<typeof gapKindSchema>;
export type FindingKind = z.infer<typeof findingKindSchema>;
export type NodeRef = z.infer<typeof nodeRefSchema>;
export type SettledLine = z.infer<typeof settledLineSchema>;
export type TraceDoc = z.infer<typeof traceDocSchema>;
export type CoverRef = z.infer<typeof coverRefSchema>;
export type TraceHeading = z.infer<typeof traceHeadingSchema>;
export type TraceEntry = z.infer<typeof traceEntrySchema>;
export type FindingStatus = z.infer<typeof findingStatusSchema>;
export type Finding = z.infer<typeof findingSchema>;
export type TraceFinding = z.infer<typeof traceFindingSchema>;
export type Gap = z.infer<typeof gapSchema>;
export type TraceProblem = z.infer<typeof traceProblemSchema>;
export type TraceView = z.infer<typeof traceViewSchema>;
export type SettledVersion = z.infer<typeof settledVersionSchema>;
export type SettleResult = z.infer<typeof settleResultSchema>;
export type SettleCommitStart = z.infer<typeof settleCommitStartSchema>;
export type SettleCommitOutcome = z.infer<typeof settleCommitOutcomeSchema>;
export type Unresolved = z.infer<typeof unresolvedSchema>;
