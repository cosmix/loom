/**
 * Object URLs of the images the user attached in this tab, by upload id, with the number of chips
 * holding each. The same file uploads to the same id, so two chips can share one URL. The chip
 * shows its thumbnail from the local file; nothing is fetched back from the server. A chip restored
 * after a reload has no entry and shows no thumbnail.
 */
const previews = new Map<string, { url: string; holders: number }>();

/**
 * Takes one hold on the thumbnail of an image just uploaded. A sent message's chip keeps showing
 * the thumbnail, so a hold is never released by sending; removing a pending chip releases it, and
 * so does a duplicate upload that `addAttachmentAtom` drops.
 */
export function rememberPreview(id: string, file: File): void {
  const known = previews.get(id);
  if (known) {
    known.holders += 1;
    return;
  }
  // A thumbnail is a nicety: an environment without object URLs uploads the file all the same.
  if (!file.type.startsWith("image/") || typeof URL.createObjectURL !== "function") return;
  previews.set(id, { url: URL.createObjectURL(file), holders: 1 });
}

export function previewUrl(id: string): string | null {
  return previews.get(id)?.url ?? null;
}

/** Releases a removed chip's hold; the URL is revoked once no chip holds it. */
export function forgetPreview(id: string): void {
  const entry = previews.get(id);
  if (entry === undefined) return;
  entry.holders -= 1;
  if (entry.holders > 0) return;
  URL.revokeObjectURL(entry.url);
  previews.delete(id);
}
