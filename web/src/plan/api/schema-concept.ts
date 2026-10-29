import { z } from "@/api/zod";
import { isConceptPageKey, isPageDoc } from "@/plan/api/page-key";

import { planStateViewSchema } from "./schema-files";

export const SUMMARY_STATES = ["current", "incomplete", "stale", "rebuilding", "missing"] as const;
export const OUT_OF_DATE_REASONS = [
  "source_changed",
  "rebuilding",
  "awaiting_approval",
  "cannot_rebuild",
  "failed",
] as const;
export const EMPTY_KINDS = [
  "no_vision",
  "sections_no_vision",
  "no_metaplan",
  "plans_no_metaplan",
  "never_built",
  "building",
] as const;

/** The empty-state offers the server sends (`Offer::id` of the two start offers). */
export const EMPTY_OFFERS = ["start-vision", "start-metaplan"] as const;

const u8 = z.number().int().min(0).max(255);
const u32 = z.number().int().min(0).max(4294967295);

const pageKeySchema = z.string().refine(isConceptPageKey, "not a concept page key");
const pageNameSchema = z.string().refine(isPageDoc, "not a page name");

export const summaryStateViewSchema = z.enum(SUMMARY_STATES);
export const outOfDateReasonSchema = z.enum(OUT_OF_DATE_REASONS);
export const emptyKindSchema = z.enum(EMPTY_KINDS);
export const outOfDateSchema = z.strictObject({
  reason: outOfDateReasonSchema,
  detail: z.string().nullable(),
});
export const docLinkSchema = z.strictObject({
  id: z.string(),
  title: z.string(),
  path: z.string(),
  page: pageKeySchema,
});
export const keyChipSchema = z.strictObject({
  doc: docLinkSchema,
  colour: u8,
  label: z.string(),
  out_of_date: outOfDateSchema.nullable(),
  no_plan: z.boolean(),
});
export const markSchema = z.strictObject({
  doc: docLinkSchema,
  colour: u8,
  out_of_date: outOfDateSchema.nullable(),
});
export const citationSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("sentence"), summary: z.string(), id: z.string() }),
  z.strictObject({ kind: z.literal("source"), path: z.string(), anchor: z.string() }),
]);
export const sourceAnchorSchema = z.strictObject({ path: z.string(), anchor: z.string() });
export const planLinkSchema = z.strictObject({
  slug: pageNameSchema,
  title: z.string(),
  state: z.string(),
});
export const sentenceSchema = z.strictObject({
  id: z.string(),
  text: z.string(),
  marks: z.array(markSchema),
  citations: z.array(citationSchema),
  rests_on: z.array(sourceAnchorSchema),
  plans: z.array(planLinkSchema),
});
export const blockSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("heading"), level: u8, text: z.string() }),
  z.strictObject({ kind: z.literal("paragraph"), sentences: z.array(sentenceSchema) }),
]);
export const summaryViewSchema = z.strictObject({
  state: summaryStateViewSchema,
  out_of_date: outOfDateSchema.nullable(),
});
export const sectionLinkSchema = z.strictObject({
  number: u32.nullable(),
  name: z.string(),
  doc: docLinkSchema.nullable(),
});
export const implementsViewSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("not_stated") }),
  z.strictObject({ kind: z.literal("sections"), links: z.array(sectionLinkSchema) }),
]);
export const emptyStateSchema = z.strictObject({
  kind: emptyKindSchema,
  count: u32.nullable(),
  offer: z.enum(EMPTY_OFFERS).nullable(),
  docs: z.array(docLinkSchema),
});
export const progressSchema = z.strictObject({ done: u32, total: u32 });
export const conceptPageSchema = z.strictObject({
  feature: z.string().nullable(),
  key: z.string(),
  title: z.string(),
  path: z.string().nullable(),
  file_hash: z.string().nullable(),
  colour: u8.nullable(),
  state: summaryViewSchema.nullable(),
  plan_state: planStateViewSchema.nullable(),
  last_pressure: z.string().nullable(),
  key_chips: z.array(keyChipSchema),
  blocks: z.array(blockSchema),
  unlisted: z.array(docLinkSchema),
  implements: implementsViewSchema.nullable(),
  implemented_by: z.array(planLinkSchema),
  empty: emptyStateSchema.nullable(),
  building: progressSchema.nullable(),
  estimate: z.string().nullable(),
});
export const conceptChangedSchema = z.strictObject({
  feature: z.string().nullable(),
  pages: z.array(z.string()),
  out_of_date: z.array(z.string()),
  sources: z.array(z.string()),
});
export const conceptMessageSchemas = [
  z.strictObject({ type: z.literal("concept_changed"), ...conceptChangedSchema.shape }),
] as const;

export type ConceptPage = z.infer<typeof conceptPageSchema>;
export type KeyChip = z.infer<typeof keyChipSchema>;
export type DocLink = z.infer<typeof docLinkSchema>;
export type Block = z.infer<typeof blockSchema>;
export type Sentence = z.infer<typeof sentenceSchema>;
export type Mark = z.infer<typeof markSchema>;
export type Citation = z.infer<typeof citationSchema>;
export type SourceAnchor = z.infer<typeof sourceAnchorSchema>;
export type SummaryView = z.infer<typeof summaryViewSchema>;
export type SummaryStateView = z.infer<typeof summaryStateViewSchema>;
export type OutOfDate = z.infer<typeof outOfDateSchema>;
export type OutOfDateReason = z.infer<typeof outOfDateReasonSchema>;
export type ImplementsView = z.infer<typeof implementsViewSchema>;
export type SectionLink = z.infer<typeof sectionLinkSchema>;
export type PlanLink = z.infer<typeof planLinkSchema>;
export type EmptyState = z.infer<typeof emptyStateSchema>;
export type EmptyKind = z.infer<typeof emptyKindSchema>;
export type Progress = z.infer<typeof progressSchema>;
export type ConceptChanged = z.infer<typeof conceptChangedSchema>;
