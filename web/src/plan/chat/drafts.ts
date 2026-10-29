import { atom, type Atom } from "jotai/vanilla";
import { atomWithStorage } from "jotai/utils";

import { z } from "@/api/zod";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { serverInfoAtom } from "@/plan/state/atoms";
import { storageKey, validatedStorage } from "./storage";

type Draft = { text: string; attachments: AttachmentDraft[] };

const attachmentDraftSchema = z.discriminatedUnion("kind", [
  z.strictObject({
    kind: z.literal("source"),
    path: z.string(),
    anchor: z.string().nullable(),
    text: z.string(),
    lines: z
      .strictObject({ start: z.number().int().min(0), end: z.number().int().min(0) })
      .nullable(),
    stageId: z.string().nullable(),
    hash: z.string().nullable(),
  }),
  z.strictObject({
    kind: z.literal("summary"),
    summary: z.string(),
    sentence: z.string(),
    text: z.string(),
    restsOn: z.array(z.strictObject({ path: z.string(), anchor: z.string() })),
    hash: z.string().nullable(),
  }),
]);

const draftSchema = z.strictObject({
  text: z.string(),
  attachments: z.array(attachmentDraftSchema),
});
const emptyDraft: Draft = { text: "", attachments: [] };

export const MAX_ATTACHMENTS = 10;
export const ATTACHMENT_LIMIT_NOTE = `At most ${MAX_ATTACHMENTS} attachments per message`;

/** The saved attachments, then each pending one that is not already saved (compared by content). */
export function mergeAttachments(
  saved: AttachmentDraft[],
  pending: AttachmentDraft[],
): AttachmentDraft[] {
  return [
    ...saved,
    ...pending.filter((item) => !saved.some((s) => JSON.stringify(s) === JSON.stringify(item))),
  ];
}

const draftStorage = validatedStorage(draftSchema);
const drafts = new Map<string, ReturnType<typeof atomWithStorage<Draft>>>();

function draftFor(key: string) {
  let value = drafts.get(key);
  if (!value) {
    value = atomWithStorage<Draft>(key, emptyDraft, draftStorage, { getOnInit: true });
    drafts.set(key, value);
  }
  return value;
}

/**
 * Text typed before the first hello, when there is no project key. It stays in memory (a bare
 * storage key would be shared by every project on the origin) and joins the keyed draft once the
 * key arrives.
 */
const unkeyedDraftAtom = atom<Draft>(emptyDraft);
const isEmpty = (draft: Draft) => draft.text === "" && draft.attachments.length === 0;

function joined(stored: Draft, held: Draft): Draft {
  if (isEmpty(held)) return stored;
  if (isEmpty(stored)) return held;
  return {
    text: [stored.text, held.text].filter(Boolean).join("\n\n"),
    attachments: [...stored.attachments, ...held.attachments],
  };
}

const keyedDraftAtom = (projectKey: string) => draftFor(storageKey(projectKey, "draft"));

function currentDraft(get: <T>(target: Atom<T>) => T): Draft {
  const key = get(serverInfoAtom)?.project_key;
  const held = get(unkeyedDraftAtom);
  return key == null ? held : joined(get(keyedDraftAtom(key)), held);
}

/**
 * Reads show the keyed draft joined with any held pre-hello text. The first write after the key
 * arrives stores the result under the key and drops the held copy, so no text is lost.
 */
export const draftAtom = atom(
  (get) => currentDraft(get),
  (get, set, update: Draft | ((previous: Draft) => Draft)) => {
    const key = get(serverInfoAtom)?.project_key;
    const next = typeof update === "function" ? update(currentDraft(get)) : update;
    if (key == null) {
      set(unkeyedDraftAtom, next);
      return;
    }
    set(keyedDraftAtom(key), next);
    set(unkeyedDraftAtom, emptyDraft);
  },
);
