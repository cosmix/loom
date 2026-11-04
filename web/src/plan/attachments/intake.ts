import { atom } from "jotai/vanilla";

import type { Store } from "@/state/store";
import { addAttachmentAtom } from "./atoms";
import type { FileDraft } from "./types";
import { uploadAttachment, uploadProblem, type UploadedFile } from "./upload";

/**
 * Object URLs of the images the user attached in this tab, by upload id. The chip shows its
 * thumbnail from the local file; nothing is fetched back from the server. A chip restored after a
 * reload has no entry and shows no thumbnail.
 */
const previews = new Map<string, string>();

export function rememberPreview(id: string, file: File): void {
  // A thumbnail is a nicety: an environment without object URLs uploads the file all the same.
  if (!file.type.startsWith("image/") || previews.has(id)) return;
  if (typeof URL.createObjectURL !== "function") return;
  previews.set(id, URL.createObjectURL(file));
}

export function previewUrl(id: string): string | null {
  return previews.get(id) ?? null;
}

/** Revokes the thumbnail's URL when its chip leaves the draft. */
export function forgetPreview(id: string): void {
  const url = previews.get(id);
  if (url === undefined) return;
  URL.revokeObjectURL(url);
  previews.delete(id);
}

export type UploadStatus = { busy: number; problems: string[] };

/** What the chat's chip list says about uploads under way and the ones that were refused. */
export const uploadStatusAtom = atom<UploadStatus>({ busy: 0, problems: [] });

export type UploadBatch = { uploaded: UploadedFile[]; problems: string[] };

/**
 * Uploads the files that fit in `room` one after another. A refused file is reported and the rest
 * still go; a file past `room` is named in a problem rather than uploaded and then dropped.
 */
export async function uploadFiles(files: File[], room: number): Promise<UploadBatch> {
  const batch: UploadBatch = { uploaded: [], problems: [] };
  for (const [index, file] of files.entries()) {
    if (index >= room) {
      batch.problems.push(`${file.name} was not attached: at most 10 attachments per message.`);
      continue;
    }
    const local = uploadProblem(file);
    if (local !== null) {
      batch.problems.push(local);
      continue;
    }
    try {
      const stored = await uploadAttachment(file);
      rememberPreview(stored.id, file);
      batch.uploaded.push(stored);
    } catch (error) {
      batch.problems.push(
        `${file.name}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  return batch;
}

/** Uploads into the chat's chip list and records progress and refusals in `uploadStatusAtom`. */
export async function attachFiles(store: Store, files: File[], room: number): Promise<void> {
  store.set(uploadStatusAtom, (status) => ({ busy: status.busy + files.length, problems: [] }));
  const { uploaded, problems } = await uploadFiles(files, room);
  for (const file of uploaded) {
    const draft: FileDraft = { kind: "file", ...file };
    store.set(addAttachmentAtom, draft);
  }
  store.set(uploadStatusAtom, (status) => ({ busy: status.busy - files.length, problems }));
}
