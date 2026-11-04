import { atom } from "jotai/vanilla";

import type { Store } from "@/state/store";
import { addAttachmentAtom } from "./atoms";
import { rememberPreview } from "./previews";
import type { FileDraft } from "./types";
import { uploadAttachment, uploadProblem, type UploadedFile } from "./upload";

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

export { forgetPreview, previewUrl, rememberPreview } from "./previews";
