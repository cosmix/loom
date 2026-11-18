import { z } from "@/api/zod";
import { apiGet, apiPost } from "@/plan/api/client";
import { planLinkSchema } from "@/plan/api/schema-concept";
import {
  openQuestionsSchema,
  reviewCommandSchema,
  reviewMessageSchema,
  suggestedPassageSchema,
} from "@/plan/api/schema-review";
import { docRel } from "@/plan/files/address";

const restoreRequestSchema = z.object({
  session: z.string(),
  turn: z.string(),
  paths: z.array(z.string()),
  force: z.boolean(),
});
const restoreFileSchema = z.discriminatedUnion("result", [
  z.object({ path: z.string(), result: z.literal("restored") }),
  z.object({ path: z.string(), result: z.literal("refused"), reason: z.string() }),
  z.object({
    path: z.string(),
    result: z.literal("changed"),
    current_text: z.string(),
    current_hash: z.string(),
    turn_start_text: z.string(),
  }),
  z.object({ path: z.string(), result: z.literal("invalid"), errors: z.array(z.string()) }),
]);
export const restoreResponseSchema = z.object({ files: z.array(restoreFileSchema) });
const hashesResponseSchema = z.object({
  head: z.string().nullable(),
  docs: z.record(z.string(), z.string()),
});
const refResponseSchema = z.object({ commit: z.string() });
const docChangeSchema = z.enum(["added", "changed", "removed"]);
const changedDocSchema = z.object({ path: z.string(), change: docChangeSchema });
export const compareResponseSchema = z.object({
  commit: z.string(),
  available: z.boolean(),
  docs: z.array(changedDocSchema),
});
const atCommitResponseSchema = z.object({ path: z.string(), text: z.string().nullable() });
const builtNoteSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("changed_after"),
    slug: z.string(),
    built: z.string(),
    base_commit: z.string().nullable(),
  }),
  z.object({ kind: z.literal("running"), slug: z.string() }),
]);
const builtResponseSchema = z.object({ notes: z.array(builtNoteSchema) });
const turnSectionsFileSchema = z.object({ path: z.string(), anchors: z.array(z.string()) });
const turnSectionsResponseSchema = z.object({
  turn: z.string(),
  files: z.array(turnSectionsFileSchema),
});
const implementedByResponseSchema = z.object({
  sections: z.record(z.string(), z.array(planLinkSchema)),
});

type RestoreRequest = z.infer<typeof restoreRequestSchema>;
export type RestoreResponse = z.infer<typeof restoreResponseSchema>;
export type TurnSectionsResponse = z.infer<typeof turnSectionsResponseSchema>;
export type HashesResponse = z.infer<typeof hashesResponseSchema>;
export type CompareResponse = z.infer<typeof compareResponseSchema>;
export type BuiltResponse = z.infer<typeof builtResponseSchema>;
export type ImplementedByResponse = z.infer<typeof implementedByResponseSchema>;
export type SuggestedPassage = z.infer<typeof suggestedPassageSchema>;
export type ReviewCommand = z.infer<typeof reviewCommandSchema>;
export type ReviewMessage = z.infer<typeof reviewMessageSchema>;

const param = encodeURIComponent;
export const postRestore = (body: RestoreRequest) =>
  apiPost("/api/restore", body, restoreResponseSchema, { contentType: "json" });
export const getTurnSections = (session: string, turn: string) =>
  apiGet(
    `/api/turn-sections?session=${param(session)}&turn=${param(turn)}`,
    turnSectionsResponseSchema,
  );
export const getHashes = () => apiGet("/api/hashes", hashesResponseSchema);
export const resolveRef = (name: string) =>
  apiGet(`/api/ref?name=${param(name)}`, refResponseSchema);
export const getCompare = (commit: string) =>
  apiGet(`/api/compare?commit=${param(commit)}`, compareResponseSchema);
export const getAtCommit = (commit: string, path: string) =>
  apiGet(`/api/at-commit?commit=${param(commit)}&path=${param(path)}`, atCommitResponseSchema);
export const getBuilt = (section: string) =>
  apiGet(`/api/built?section=${param(docRel(section))}`, builtResponseSchema);
export const getOpenQuestions = () => apiGet("/api/open-questions", openQuestionsSchema);
export const getImplementedBy = () => apiGet("/api/implemented-by", implementedByResponseSchema);
