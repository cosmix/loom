import { z } from "@/api/zod";

export const ENTRY_KINDS = [
  "vision",
  "section",
  "note",
  "metaplan",
  "metaplan_entry",
  "plan",
  "brief",
  "brief_folder",
  "knowledge",
  "knowledge_folder",
  "review",
  "other",
] as const;
export const PLAN_STATES = ["written", "running", "done", "retired"] as const;
export const LOCK_KINDS = [
  "running",
  "done",
  "archive",
  "brief_of_locked_plan",
  "knowledge",
  "summaries",
] as const;
export const METADATA_STATES = ["present", "missing"] as const;
export const SAVE_RESULTS = ["saved", "conflict", "invalid"] as const;

export const entryKindSchema = z.enum(ENTRY_KINDS);
export const planStateViewSchema = z.enum(PLAN_STATES);
export const lockKindSchema = z.enum(LOCK_KINDS);
export const metadataViewSchema = z.enum(METADATA_STATES);
const u8 = z.number().int().min(0).max(255);
const u32 = z.number().int().min(0).max(4294967295);
const u64 = z.number().int().min(0);

type TreeEntry = {
  path: string | null;
  title: string;
  file_name: string | null;
  kind: z.infer<typeof entryKindSchema>;
  state: z.infer<typeof planStateViewSchema> | null;
  locked: boolean;
  gap: boolean;
  pressure_date: string | null;
  errors: number;
  blurb: string | null;
  children: TreeEntry[];
};
export const treeEntrySchema: z.ZodType<TreeEntry> = z.strictObject({
  path: z.string().nullable(),
  title: z.string(),
  file_name: z.string().nullable(),
  kind: entryKindSchema,
  state: planStateViewSchema.nullable(),
  locked: z.boolean(),
  gap: z.boolean(),
  pressure_date: z.string().nullable(),
  errors: u32,
  blurb: z.string().nullable(),
  children: z.array(z.lazy(() => treeEntrySchema)),
});
export const fileTreeSchema = z.strictObject({
  product: z.array(treeEntrySchema),
  plans: z.array(treeEntrySchema),
  knowledge: z.array(treeEntrySchema),
  other: z.array(treeEntrySchema),
  archive: z.array(treeEntrySchema),
});
export const sectionViewSchema = z.strictObject({
  level: u8,
  title: z.string(),
  anchor: z.string(),
  start_line: u32,
  end_line: u32,
});
export const lockViewSchema = z.strictObject({
  reason: lockKindSchema,
  notice: z.string(),
  plan: z.string().nullable(),
});
export const implementsLinkSchema = z.strictObject({ number: u32, path: z.string().nullable() });
export const stageRowSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  stage_type: z.string(),
  summary: z.string().nullable(),
  dependencies: z.array(z.string()),
  description: z.string(),
  acceptance: z.array(z.string()),
  files: z.array(z.string()),
  line: u32.nullable(),
});
export const validationItemSchema = z.strictObject({
  message: z.string(),
  stage_id: z.string().nullable(),
  line: u32.nullable(),
  column: u32.nullable(),
});
export const validationWarningsSchema = z.strictObject({
  structural: z.array(z.string()),
  knowledge: z.array(z.string()),
  sandbox: z.array(z.string()),
  baseline: z.array(z.string()),
});
export const validationViewSchema = z.strictObject({
  valid: z.boolean(),
  errors: z.array(validationItemSchema),
  warnings: validationWarningsSchema,
  notes: z.array(z.string()),
  metadata: metadataViewSchema,
});
export const planViewSchema = z.strictObject({
  slug: z.string(),
  state: planStateViewSchema,
  implements: z.array(implementsLinkSchema),
  pressure_date: z.string().nullable(),
  last_change: z.string().nullable(),
  validation: validationViewSchema.nullable(),
  stages: z.array(stageRowSchema),
  yaml: z.string().nullable(),
  yaml_error: z.string().nullable(),
  metadata_line: u32.nullable(),
});
export const docViewSchema = z.strictObject({
  path: z.string(),
  title: z.string(),
  text: z.string(),
  hash: z.string(),
  kind: entryKindSchema,
  editable: z.boolean(),
  lock: lockViewSchema.nullable(),
  sections: z.array(sectionViewSchema),
  plan: planViewSchema.nullable(),
  too_large: z.boolean(),
  truncated: z.boolean(),
  size: u64,
});
export const saveResultSchema = z.discriminatedUnion("result", [
  z.strictObject({
    result: z.literal("saved"),
    hash: z.string(),
    validation: validationViewSchema.nullable(),
  }),
  z.strictObject({
    result: z.literal("conflict"),
    current_text: z.string(),
    current_hash: z.string(),
  }),
  z.strictObject({ result: z.literal("invalid"), validation: validationViewSchema }),
]);
export const diffTextsSchema = z.strictObject({
  path: z.string(),
  before: z.string().nullable(),
  after: z.string().nullable(),
});
export const searchHitSchema = z.strictObject({
  path: z.string(),
  title: z.string(),
  heading_path: z.array(z.string()),
  anchor: z.string(),
  snippet: z.string(),
  terms: z.array(z.string()),
});
export const searchResultsSchema = z.strictObject({
  hits: z.array(searchHitSchema),
  truncated: z.boolean(),
});
export const docChangedSchema = z.strictObject({ path: z.string(), hash: z.string().nullable() });
export const validationChangedSchema = z.strictObject({
  path: z.string(),
  validation: validationViewSchema,
});

export const filesMessageSchemas = [
  z.strictObject({ type: z.literal("tree_changed"), ...fileTreeSchema.shape }),
  z.strictObject({ type: z.literal("doc_changed"), ...docChangedSchema.shape }),
  z.strictObject({ type: z.literal("validation"), ...validationChangedSchema.shape }),
] as const;

export type FileTree = z.infer<typeof fileTreeSchema>;
export type { TreeEntry };
export type EntryKind = z.infer<typeof entryKindSchema>;
export type PlanStateView = z.infer<typeof planStateViewSchema>;
export type DocView = z.infer<typeof docViewSchema>;
export type SectionView = z.infer<typeof sectionViewSchema>;
export type LockView = z.infer<typeof lockViewSchema>;
export type LockKind = z.infer<typeof lockKindSchema>;
export type PlanView = z.infer<typeof planViewSchema>;
export type ImplementsLink = z.infer<typeof implementsLinkSchema>;
export type StageRow = z.infer<typeof stageRowSchema>;
export type ValidationView = z.infer<typeof validationViewSchema>;
export type ValidationItem = z.infer<typeof validationItemSchema>;
export type ValidationWarnings = z.infer<typeof validationWarningsSchema>;
export type MetadataView = z.infer<typeof metadataViewSchema>;
export type SaveResult = z.infer<typeof saveResultSchema>;
export type DiffTexts = z.infer<typeof diffTextsSchema>;
export type SearchResults = z.infer<typeof searchResultsSchema>;
export type SearchHit = z.infer<typeof searchHitSchema>;
export type DocChanged = z.infer<typeof docChangedSchema>;
export type ValidationChanged = z.infer<typeof validationChangedSchema>;
