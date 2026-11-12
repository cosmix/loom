import { parseMarkdown } from "@/plan/markdown/markdown";

export type DiffBlock = {
  kind: "same" | "added" | "removed" | "changed";
  before: string | null;
  after: string | null;
};

/** A block's source text and the comparison key derived from it. */
type Block = { text: string; key: string };

/**
 * Prose reflows, so runs of whitespace in a paragraph or heading do not count as a change. Every
 * other node (code, lists, tables, html) is whitespace-significant: indentation edits show, and
 * only line endings and trailing blanks are normalised.
 */
function keyOf(type: string, text: string): string {
  if (type === "paragraph" || type === "heading") return text.trim().replace(/\s+/g, " ");
  return text
    .replace(/\r\n?/g, "\n")
    .replace(/[ \t]+$/gm, "")
    .trim();
}

function blocksOf(source: string): Block[] {
  const blocks: Block[] = [];
  for (const node of parseMarkdown(source).children) {
    const start = node.position?.start.offset;
    const end = node.position?.end.offset;
    if (start === undefined || end === undefined) continue;
    const text = source.slice(start, end);
    blocks.push({ text, key: keyOf(node.type, text) });
  }
  return blocks;
}

function lcsTable(before: string[], after: string[]): Uint32Array[] {
  const table = Array.from({ length: before.length + 1 }, () => new Uint32Array(after.length + 1));
  for (let i = before.length - 1; i >= 0; i--) {
    for (let j = after.length - 1; j >= 0; j--) {
      table[i][j] =
        before[i] === after[j]
          ? table[i + 1][j + 1] + 1
          : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  return table;
}

function alignBlocks(before: Block[], after: Block[]): DiffBlock[] {
  const beforeKeys = before.map((block) => block.key);
  const afterKeys = after.map((block) => block.key);
  const table = lcsTable(beforeKeys, afterKeys);
  const blocks: DiffBlock[] = [];
  let i = 0;
  let j = 0;
  while (i < before.length || j < after.length) {
    if (i < before.length && j < after.length && beforeKeys[i] === afterKeys[j]) {
      blocks.push({ kind: "same", before: before[i++].text, after: after[j++].text });
    } else if (i < before.length && (j === after.length || table[i + 1][j] >= table[i][j + 1])) {
      // Prefer removals on ties so replacement runs put removed blocks before added blocks.
      blocks.push({ kind: "removed", before: before[i++].text, after: null });
    } else {
      blocks.push({ kind: "added", before: null, after: after[j++].text });
    }
  }
  return blocks;
}

function pairChanges(blocks: DiffBlock[]): DiffBlock[] {
  const result: DiffBlock[] = [];
  let i = 0;
  while (i < blocks.length) {
    if (blocks[i].kind !== "removed") {
      result.push(blocks[i++]);
      continue;
    }
    const removedStart = i;
    while (i < blocks.length && blocks[i].kind === "removed") i++;
    const addedStart = i;
    while (i < blocks.length && blocks[i].kind === "added") i++;
    const count = Math.max(addedStart - removedStart, i - addedStart);
    for (let offset = 0; offset < count; offset++) {
      const removed =
        offset < addedStart - removedStart ? blocks[removedStart + offset] : undefined;
      const added = offset < i - addedStart ? blocks[addedStart + offset] : undefined;
      result.push({
        kind: removed && added ? "changed" : removed ? "removed" : "added",
        before: removed?.before ?? null,
        after: added?.after ?? null,
      });
    }
  }
  return result;
}

function indexedDiff(before: Block[], after: Block[]): DiffBlock[] {
  return Array.from({ length: Math.max(before.length, after.length) }, (_, i) => ({
    kind:
      i >= before.length
        ? "added"
        : i >= after.length
          ? "removed"
          : before[i].key === after[i].key
            ? "same"
            : "changed",
    before: before[i]?.text ?? null,
    after: after[i]?.text ?? null,
  }));
}

export function diffBlocks(before: string, after: string): DiffBlock[] {
  const beforeBlocks = blocksOf(before);
  const afterBlocks = blocksOf(after);
  // Bound the quadratic LCS table and work for unusually large documents.
  if (beforeBlocks.length > 2_000 || afterBlocks.length > 2_000) {
    return indexedDiff(beforeBlocks, afterBlocks);
  }
  return pairChanges(alignBlocks(beforeBlocks, afterBlocks));
}
