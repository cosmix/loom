import { z } from "@/api/zod";

/** Largest upload the server takes (D9); the browser refuses a bigger file before sending it. */
export const MAX_UPLOAD_BYTES = 10 * 1024 * 1024;

/** The media types the server accepts, each with the extension a file input offers for it. */
const TYPES: Readonly<Record<string, readonly string[]>> = {
  "image/png": [".png"],
  "image/jpeg": [".jpg", ".jpeg"],
  "image/gif": [".gif"],
  "image/webp": [".webp"],
  "application/pdf": [".pdf"],
  "text/plain": [".txt"],
  "text/markdown": [".md"],
};

/** The `accept` list of the file input: extensions and types, so every picker narrows the same. */
export const UPLOAD_ACCEPT = Object.entries(TYPES)
  .flatMap(([type, extensions]) => [type, ...extensions])
  .join(",");

/** Browsers leave `File.type` empty for Markdown and sometimes for text; the extension decides. */
const BY_EXTENSION: Readonly<Record<string, string>> = { md: "text/markdown", txt: "text/plain" };

export const uploadedFileSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  media_type: z.string(),
  bytes: z.number().int().min(0),
});
export type UploadedFile = z.infer<typeof uploadedFileSchema>;

const errorSchema = z.object({ error: z.string(), message: z.string().optional() });

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/**
 * The server keeps the first 120 characters of a name but refuses one over 1024 bytes once decoded,
 * so a long name is cut here, by code point, before it is sent.
 */
const NAME_LIMIT = 120;

function sentName(name: string): string {
  return Array.from(name).slice(0, NAME_LIMIT).join("");
}

/** The media type to send for this file, or null when the server would not take it. */
export function uploadType(file: Pick<File, "name" | "type">): string | null {
  const declared = file.type.split(";")[0]?.trim().toLowerCase() ?? "";
  const type = declared === "" ? (BY_EXTENSION[extensionOf(file.name)] ?? "") : declared;
  return Object.hasOwn(TYPES, type) ? type : null;
}

/** Why this file is not sent, or null. The caller shows the reason through `<Untrusted>`. */
export function uploadProblem(file: File): string | null {
  if (uploadType(file) === null) {
    return `${file.name} is not a supported type. Attach a PNG, JPEG, GIF, WebP, PDF, text or Markdown file.`;
  }
  if (file.size > MAX_UPLOAD_BYTES) {
    return `${file.name} is over the 10 MiB limit.`;
  }
  return null;
}

async function reasonOf(response: Response): Promise<string> {
  try {
    const parsed = errorSchema.safeParse(await response.json());
    if (parsed.success) return parsed.data.message ?? parsed.data.error;
  } catch {
    // The body was not JSON; the status below is the reason.
  }
  return `HTTP ${response.status}`;
}

/**
 * Stores the file on the server and returns the id the chat sends in its place. It rejects with the
 * reason: a local refusal before sending, or the server's own wording for 413, 415, 422 and 507.
 */
export async function uploadAttachment(file: File): Promise<UploadedFile> {
  const problem = uploadProblem(file);
  if (problem !== null) throw new Error(problem);
  const contentType = uploadType(file)!;
  let response: Response;
  try {
    response = await fetch(`/api/attachments?name=${encodeURIComponent(sentName(file.name))}`, {
      method: "POST",
      headers: { "Content-Type": contentType },
      body: file,
      credentials: "same-origin",
    });
  } catch (error) {
    throw new Error(error instanceof Error ? error.message : String(error), { cause: error });
  }
  if (!response.ok) throw new Error(await reasonOf(response));
  const parsed = uploadedFileSchema.safeParse(await response.json().catch(() => undefined));
  if (!parsed.success) throw new Error("Invalid server response");
  return parsed.data;
}
