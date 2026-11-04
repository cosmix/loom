import { docRel, toAddress } from "@/plan/files/address";
import { isFeatureName } from "@/plan/new/feature-name";

/** What a source address may carry beyond the path: the lines to highlight and the heading anchor. */
export interface SourceOptions {
  lines?: { start: number; end: number } | null;
  anchor?: string | null;
}

/**
 * The feature a repository path belongs to: its vision and sections (`doc/product/<f>/…`) and its
 * metaplans and plans (`doc/plans/<f>/…`). Null for anything else, including a standalone plan
 * (`doc/plans/PLAN-x.md`, one level under plans) and a path with an empty or dot segment.
 */
export function featureOfDocPath(repoPath: string): string | null {
  const [root, feature, ...rest] = docRel(repoPath).split("/");
  if (root !== "product" && root !== "plans") return null;
  if (feature === undefined || !isFeatureName(feature) || rest.length === 0) return null;
  return rest.every((part) => part !== "" && part !== "." && part !== "..") ? feature : null;
}

function suffix(options: SourceOptions): string {
  const query = options.lines ? `?lines=${options.lines.start}-${options.lines.end}` : "";
  const hash = options.anchor ? `#${encodeURIComponent(options.anchor)}` : "";
  return `${query}${hash}`;
}

/**
 * `/f/<feature>/source/<repo path without doc/>`: a feature document's source, spine included.
 * With no feature (a path outside every feature) it is the file-mode address of the path.
 */
export function featureSourceHref(
  feature: string | null,
  repoPath: string,
  options: SourceOptions = {},
): string {
  if (feature === null) return `${toAddress(repoPath)}${suffix(options)}`;
  const rel = docRel(repoPath).split("/").map(encodeURIComponent).join("/");
  return `/f/${encodeURIComponent(feature)}/source/${rel}${suffix(options)}`;
}

/**
 * Where a document's source opens from a feature page: inside its feature when the path belongs to
 * one, in file mode otherwise.
 */
export function sourceHref(repoPath: string, options: SourceOptions = {}): string {
  return featureSourceHref(featureOfDocPath(repoPath), repoPath, options);
}
