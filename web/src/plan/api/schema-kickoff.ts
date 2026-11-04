import { z } from "@/api/zod";
import { apiGet } from "@/plan/api/client";

const u64 = z.number().int().min(0);

export const SHAPE_STATES = ["settled", "wording", "open"] as const;
export const KICKOFF_SIZES = ["feature", "plan"] as const;
export const TEAMMATE_STATES = ["working", "idle"] as const;

export const shapeStateSchema = z.enum(SHAPE_STATES);
export const kickoffSizeSchema = z.enum(KICKOFF_SIZES);
export const shapeSectionSchema = z.strictObject({
  title: z.string(),
  state: shapeStateSchema,
});
export const kickoffShapeSchema = z.strictObject({
  size: kickoffSizeSchema.nullable(),
  name: z.string().nullable(),
  proposal: z.boolean(),
  problem: z.string(),
  decisions: z.array(z.string()),
  questions: z.array(z.string()),
  sections: z.array(shapeSectionSchema),
});
export const kickoffViewSchema = z.strictObject({
  session: z.string(),
  shape: kickoffShapeSchema,
  target: z.string().nullable(),
});
// One unfinished kickoff as the server lists it. The detail a draft view needs (decisions,
// questions, the typed messages) comes from `GET /api/kickoffs/<id>`, not from this entry.
export const kickoffDraftViewSchema = z.strictObject({
  session: z.string(),
  name: z.string().nullable(),
  size: kickoffSizeSchema.nullable(),
  problem: z.string().nullable(),
  started_at: z.string(),
  updated_at: z.string(),
  live: z.boolean(),
});
export const kickoffDraftDetailSchema = z.strictObject({
  draft: kickoffDraftViewSchema,
  shape: kickoffShapeSchema.nullable(),
  messages: z.array(z.string()),
});
export const tokenCountsSchema = z.strictObject({
  input: u64,
  cache_write: u64,
  cache_read: u64,
  output: u64,
});
export const teammateStatusSchema = z.strictObject({
  agent_id: z.string(),
  name: z.string(),
  documents: z.array(z.string()),
  current: z.string().nullable(),
  state: z.enum(TEAMMATE_STATES),
  tokens: tokenCountsSchema,
});
export const draftTeamViewSchema = z.strictObject({
  session: z.string(),
  documents: z.array(z.string()),
  teammates: z.array(teammateStatusSchema),
});
export const kickoffMessageSchemas = [
  z.strictObject({ type: z.literal("kickoff"), ...kickoffViewSchema.shape }),
  z.strictObject({ type: z.literal("draft_team"), ...draftTeamViewSchema.shape }),
  z.strictObject({ type: z.literal("kickoff_drafts"), drafts: z.array(kickoffDraftViewSchema) }),
] as const;

export type ShapeState = z.infer<typeof shapeStateSchema>;
export type KickoffSize = z.infer<typeof kickoffSizeSchema>;
export type ShapeSection = z.infer<typeof shapeSectionSchema>;
export type KickoffShape = z.infer<typeof kickoffShapeSchema>;
export type KickoffView = z.infer<typeof kickoffViewSchema>;
export type KickoffDraftView = z.infer<typeof kickoffDraftViewSchema>;
export type KickoffDraftDetail = z.infer<typeof kickoffDraftDetailSchema>;
export type TokenCounts = z.infer<typeof tokenCountsSchema>;
export type TeammateStatus = z.infer<typeof teammateStatusSchema>;
export type DraftTeamView = z.infer<typeof draftTeamViewSchema>;

/** One draft with the detail the list entry leaves out; a 404 means the draft is gone. */
export function getKickoffDraft(session: string) {
  return apiGet(`/api/kickoffs/${encodeURIComponent(session)}`, kickoffDraftDetailSchema);
}
