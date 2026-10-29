import { z } from "@/api/zod";

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
] as const;

export type ShapeState = z.infer<typeof shapeStateSchema>;
export type KickoffSize = z.infer<typeof kickoffSizeSchema>;
export type ShapeSection = z.infer<typeof shapeSectionSchema>;
export type KickoffShape = z.infer<typeof kickoffShapeSchema>;
export type KickoffView = z.infer<typeof kickoffViewSchema>;
export type TokenCounts = z.infer<typeof tokenCountsSchema>;
export type TeammateStatus = z.infer<typeof teammateStatusSchema>;
export type DraftTeamView = z.infer<typeof draftTeamViewSchema>;
