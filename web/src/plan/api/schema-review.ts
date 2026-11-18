import { z } from "@/api/zod";

export const openQuestionsSchema = z.object({
  counts: z.record(z.string(), z.number().int().nonnegative()),
});
export const suggestedPassageSchema = z.object({
  path: z.string(),
  anchor: z.string(),
  text: z.string(),
  lines: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]).nullable(),
  stage: z.string().nullable(),
  summary_sentence: z.string().nullable(),
});
export const suggestionActionSchema = z.enum(["attach", "dismiss"]);
export const reviewCommandSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("suggest"), passage: suggestedPassageSchema, note: z.string() }),
  z.object({
    kind: z.literal("suggestion_answer"),
    id: z.string(),
    action: suggestionActionSchema,
  }),
]);
export const suggestionStatusSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("sent"), holder: z.string(), away: z.boolean() }),
  z.object({ state: z.literal("attached") }),
  z.object({ state: z.literal("dismissed") }),
  z.object({ state: z.literal("dropped"), reason: z.string() }),
]);
export const reviewMessageSchema = z.union([
  z.object({
    kind: z.literal("suggestion"),
    id: z.string(),
    from: z.string(),
    passage: suggestedPassageSchema,
    note: z.string(),
  }),
  z.object({ kind: z.literal("suggestion_status"), id: z.string() }).and(suggestionStatusSchema),
  z.object({ kind: z.literal("open_questions"), counts: openQuestionsSchema.shape.counts }),
  z.object({ kind: z.literal("refused"), reason: z.string() }),
]);

/** Plan 9's frames beside the tag: the server flattens a review message next to `type`. */
export const reviewWireMessageSchema = z
  .object({ type: z.literal("review") })
  .and(reviewMessageSchema);
export const reviewWireCommandSchema = z
  .object({ type: z.literal("review") })
  .and(reviewCommandSchema);
