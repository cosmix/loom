import { z } from "@/api/zod";
import { chatMessageSchema, chatWireCommandSchema } from "./schema-chat";
import { conceptMessageSchemas } from "./schema-concept";
import { filesMessageSchemas, planStateViewSchema } from "./schema-files";
import { kickoffMessageSchemas } from "./schema-kickoff";
import { pressureMessageSchemas } from "./schema-pressure";
import { runMessageSchemas } from "./schema-run";
import { reviewWireCommandSchema, reviewWireMessageSchema } from "./schema-review";
import { summaryMessageSchemas } from "./schema-summaries";

export const PREREQ_KINDS = ["tmux", "claude", "claude_login", "sandbox"] as const;
export const BIND_KINDS = ["loopback", "remote"] as const;
export const STEP_MARKS = ["done", "gap", "active", "optional"] as const;
export const CURRENT_KINDS = ["step", "ready", "ready_all_done"] as const;
export const OFFER_KINDS = [
  "start_vision",
  "fix_vision_table",
  "write_section",
  "add_section_to_vision",
  "start_metaplan",
  "write_plan",
  "pressure_test",
  "start_run",
  "plan_follow_ups",
  "draft_all_sections",
  "draft_all_plans",
  "move_into_feature",
] as const;
export const PLAN_MESSAGE_TYPES = [
  "hello",
  "presence",
  "authoring",
  "checklist",
  "server_stopping",
  "refused",
  "tree_changed",
  "doc_changed",
  "validation",
  "concept_changed",
  "summary_panel",
  "summary_question",
  "summary_question_closed",
  "summary_turn_cost",
  "pressure_run",
  "pressure_options_changed",
  "run_state",
  "run_output",
  "run_strip",
  "kickoff",
  "draft_team",
  "kickoff_drafts",
] as const;
export const CLIENT_MESSAGE_TYPES = ["set_name", "reading"] as const;

export const prereqKindSchema = z.enum(PREREQ_KINDS);
export const bindKindSchema = z.enum(BIND_KINDS);
export const markViewSchema = z.enum(STEP_MARKS);
export const offerKindSchema = z.enum(OFFER_KINDS);
export const youSchema = z.strictObject({ public_id: z.string(), name: z.string() });
export const missingPrereqSchema = z.strictObject({
  what: prereqKindSchema,
  message: z.string(),
  fix: z.string(),
});
export const serverInfoSchema = z.strictObject({
  project: z.string(),
  project_key: z.string(),
  port: z.number().int().min(0).max(65535),
  remote: z.boolean(),
  read_only: z.array(missingPrereqSchema),
  stopping: z.boolean(),
  you: youSchema,
});
export const shareLinkSchema = z.strictObject({
  url: z.string(),
  bind: bindKindSchema,
});
export const refusedSchema = z.strictObject({ command: z.string(), reason: z.string() });
export const presenceEntrySchema = z.strictObject({
  public_id: z.string(),
  name: z.string(),
  holder: z.boolean(),
  away: z.boolean(),
  reading: z.string().nullable(),
  tabs: z.number().int().min(0).max(4294967295),
});
export const presenceListSchema = z.strictObject({ entries: z.array(presenceEntrySchema) });
export const holderViewSchema = z.strictObject({
  public_id: z.string(),
  name: z.string(),
  away_since: z.string().nullable(),
  frees_at: z.string().nullable(),
});
export const authoringViewSchema = z.strictObject({ holder: holderViewSchema.nullable() });
const u8 = z.number().int().min(0).max(255);
export const stepViewSchema = z.strictObject({
  number: u8,
  name: z.string(),
  mark: markViewSchema,
  count: z.string().nullable(),
  label: z.string(),
});
export const currentViewSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("step"), number: u8 }),
  z.strictObject({ kind: z.literal("ready") }),
  z.strictObject({ kind: z.literal("ready_all_done") }),
]);
export const offerViewSchema = z.strictObject({
  id: z.string(),
  kind: offerKindSchema,
  label: z.string(),
  target: z.string().nullable(),
  feature: z.string().optional(),
  number: z.number().int().min(0).max(4294967295).optional(),
  slug: z.string().optional(),
  metaplan: z.string().optional(),
});
export const itemViewSchema = z.strictObject({
  step: u8,
  text: z.string(),
  path: z.string().nullable(),
  offer: offerViewSchema.nullable(),
});
const u32 = z.number().int().min(0).max(4294967295);
export const sectionViewSchema = z.strictObject({
  number: z.string(),
  name: z.string(),
  written: z.boolean(),
  open_questions: u32,
});
export const planCountsViewSchema = z.strictObject({
  listed: u32,
  total: u32,
  written: u32,
  running: u32,
  done: u32,
});
export const metaplanRowViewSchema = z.strictObject({ path: z.string(), title: z.string() });
export const planRowViewSchema = z.strictObject({
  slug: z.string(),
  path: z.string(),
  title: z.string(),
  state: z.string(),
  pressure_date: z.string().nullable(),
  listed: z.boolean(),
});
export const checklistViewSchema = z.strictObject({
  feature: z.string(),
  pitch: z.string().nullable(),
  has_product: z.boolean(),
  sections: z.array(sectionViewSchema),
  plans: planCountsViewSchema,
  untested: u32,
  metaplans: z.array(metaplanRowViewSchema),
  plan_rows: z.array(planRowViewSchema),
  current: currentViewSchema,
  steps: z.array(stepViewSchema),
  primary: offerViewSchema.nullable(),
  secondary: z.array(offerViewSchema),
  items: z.array(itemViewSchema),
});
export const standalonePlanViewSchema = z.strictObject({
  slug: z.string(),
  path: z.string(),
  state: planStateViewSchema,
  title: z.string(),
});
export const projectChecklistViewSchema = z.strictObject({
  checklists: z.array(checklistViewSchema),
  standalone: z.array(standalonePlanViewSchema),
  moves: z.array(offerViewSchema),
});
export const helloSchema = z.strictObject({
  you: youSchema,
  server: serverInfoSchema,
  ...projectChecklistViewSchema.shape,
  presence: presenceListSchema,
  authoring: authoringViewSchema,
});
export const planMessageSchema = z.discriminatedUnion("type", [
  z.strictObject({ type: z.literal("hello"), ...helloSchema.shape }),
  z.strictObject({ type: z.literal("presence"), ...presenceListSchema.shape }),
  z.strictObject({ type: z.literal("authoring"), ...authoringViewSchema.shape }),
  z.strictObject({ type: z.literal("checklist"), ...projectChecklistViewSchema.shape }),
  z.strictObject({ type: z.literal("server_stopping") }),
  z.strictObject({ type: z.literal("refused"), ...refusedSchema.shape }),
  ...filesMessageSchemas,
  ...conceptMessageSchemas,
  ...summaryMessageSchemas,
  ...pressureMessageSchemas,
  ...runMessageSchemas,
  ...kickoffMessageSchemas,
]);
export const clientMessageSchema = z.discriminatedUnion("type", [
  z.strictObject({ type: z.literal("set_name"), name: z.string() }),
  z.strictObject({ type: z.literal("reading"), path: z.string().nullable() }),
]);

export type PrereqKind = z.infer<typeof prereqKindSchema>;
export type BindKind = z.infer<typeof bindKindSchema>;
export type MarkView = z.infer<typeof markViewSchema>;
export type OfferKind = z.infer<typeof offerKindSchema>;
export type You = z.infer<typeof youSchema>;
export type MissingPrereq = z.infer<typeof missingPrereqSchema>;
export type ServerInfo = z.infer<typeof serverInfoSchema>;
export type ShareLink = z.infer<typeof shareLinkSchema>;
export type Refused = z.infer<typeof refusedSchema>;
export type PresenceEntry = z.infer<typeof presenceEntrySchema>;
export type PresenceList = z.infer<typeof presenceListSchema>;
export type HolderView = z.infer<typeof holderViewSchema>;
export type AuthoringView = z.infer<typeof authoringViewSchema>;
export type StepView = z.infer<typeof stepViewSchema>;
export type CurrentView = z.infer<typeof currentViewSchema>;
export type OfferView = z.infer<typeof offerViewSchema>;
export type ItemView = z.infer<typeof itemViewSchema>;
export type ChecklistView = z.infer<typeof checklistViewSchema>;
export type StandalonePlanView = z.infer<typeof standalonePlanViewSchema>;
export type ProjectChecklistView = z.infer<typeof projectChecklistViewSchema>;
export type Hello = z.infer<typeof helloSchema>;
// Chat and review frames share a `type` tag across several wire shapes, so they sit beside the
// discriminated unions (which the Rust tag-array tests read) rather than inside them.
export const socketMessageSchema = z.union([
  planMessageSchema,
  chatMessageSchema,
  reviewWireMessageSchema,
]);
export const socketCommandSchema = z.union([
  clientMessageSchema,
  chatWireCommandSchema,
  reviewWireCommandSchema,
]);
export type PlanMessage = z.infer<typeof socketMessageSchema>;
export type ClientMessage = z.infer<typeof socketCommandSchema>;
