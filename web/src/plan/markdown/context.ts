import type { Definition, Root, RootContent } from "mdast";

import type { SectionView } from "@/plan/api/schema-files";

/** Everything a node renderer needs besides the node itself. */
export type RenderContext = {
  /** Repo-relative path of the document the source belongs to. */
  path: string;
  onNavigate: (address: string) => void;
  docPaths: ReadonlySet<string>;
  anchorsOf: (path: string) => ReadonlySet<string> | null;
  /** Server-owned anchor of the heading starting on this excerpt line, or null. */
  headingAnchor: (excerptLine: number) => string | null;
  /** False for embedded excerpts and chat text: no element ids, no copy control. */
  headingIds: boolean;
  /** Whole-term search highlights, or null when nothing is marked. */
  marks: RegExp | null;
  /** Link reference definitions by normalised identifier. */
  definitions: ReadonlyMap<string, Definition>;
};

export function headingAnchorLookup(
  sections: readonly SectionView[],
  lineOffset: number,
): (excerptLine: number) => string | null {
  const byLine = new Map<number, string>();
  for (const section of sections) byLine.set(section.start_line, section.anchor);
  return (excerptLine) => byLine.get(excerptLine + lineOffset) ?? null;
}

function escapeRegExp(term: string): string {
  return term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * One case-insensitive pattern for every term, matching only where the term is
 * not glued to a letter, digit or underscore on either side. Longer terms come
 * first so "plan view" wins over "plan" where both match.
 */
export function markPattern(marks: readonly string[] | undefined): RegExp | null {
  const terms = (marks ?? []).map((term) => term.trim()).filter((term) => term.length > 0);
  if (terms.length === 0) return null;
  const alternatives = [...new Set(terms)]
    .sort((a, b) => b.length - a.length)
    .map(escapeRegExp)
    .join("|");
  return new RegExp(`(?<![\\p{L}\\p{N}_])(?:${alternatives})(?![\\p{L}\\p{N}_])`, "giu");
}

export function collectDefinitions(root: Root): Map<string, Definition> {
  const found = new Map<string, Definition>();
  const visit = (nodes: readonly RootContent[]): void => {
    for (const node of nodes) {
      if (node.type === "definition") {
        if (!found.has(node.identifier)) found.set(node.identifier, node);
      } else if ("children" in node) {
        visit(node.children);
      }
    }
  };
  visit(root.children);
  return found;
}

/**
 * Raw HTML never renders: comments vanish (this hides the loom METADATA
 * markers), everything else is shown as the literal text it is. An unclosed
 * comment runs to the end of the node, as it would in a browser.
 */
export function rawHtmlText(value: string): string {
  return value.replace(/<!--[\s\S]*?(?:-->|$)/g, "").trim();
}
