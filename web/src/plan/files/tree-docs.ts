import type { FileTree, TreeEntry } from "@/plan/api/schema-files";

/** The tree's categories in drawing order, with their labels. */
export const GROUPS = [
  ["product", "Product"],
  ["plans", "Plans"],
  ["knowledge", "Knowledge"],
  ["other", "Other"],
  ["archive", "Archive"],
] as const satisfies readonly (readonly [keyof FileTree, string])[];

/** Every entry with a document behind it, nested ones included. */
export function treeDocs(tree: FileTree): TreeEntry[] {
  const found: TreeEntry[] = [];
  const visit = (entry: TreeEntry) => {
    if (entry.path !== null) found.push(entry);
    entry.children.forEach(visit);
  };
  GROUPS.forEach(([key]) => tree[key].forEach(visit));
  return found;
}

/** The paths a link may resolve to: every document in the tree, and the open one even if unlisted. */
export function docPathsOf(tree: FileTree | null, own: string): Set<string> {
  const paths = new Set([own]);
  for (const entry of tree ? treeDocs(tree) : []) {
    if (entry.path !== null) paths.add(entry.path);
  }
  return paths;
}
