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

/** Two blocks pair when their words overlap at least this much (Dice coefficient, 0 to 1). */
const PAIR_THRESHOLD = 0.5;
/**
 * Largest removed-by-added run the similarity pairing scores. A run past it is paired by index,
 * which also bounds the work after an unrelated rewrite. Raise it when real runs reach it.
 */
const MAX_PAIRING_CELLS = 10_000;

function wordsOf(text: string): Set<string> {
  return new Set(text.toLowerCase().match(/[\p{L}\p{N}_]+/gu) ?? []);
}

function overlap(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let shared = 0;
  for (const word of a) if (b.has(word)) shared++;
  return (2 * shared) / (a.size + b.size);
}

/**
 * The order-preserving pairs of removed and added blocks that maximise the total overlap, a pair
 * counting only at or above the threshold. Each pair is [removed index, added index].
 */
function similarPairs(removed: string[], added: string[]): [number, number][] {
  const removedWords = removed.map(wordsOf);
  const addedWords = added.map(wordsOf);
  const score = (i: number, j: number): number => {
    const value = overlap(removedWords[i], addedWords[j]);
    return value >= PAIR_THRESHOLD ? value : 0;
  };
  const best = Array.from({ length: removed.length + 1 }, () => new Float64Array(added.length + 1));
  for (let i = removed.length - 1; i >= 0; i--) {
    for (let j = added.length - 1; j >= 0; j--) {
      const s = score(i, j);
      best[i][j] = Math.max(best[i + 1][j], best[i][j + 1], s > 0 ? s + best[i + 1][j + 1] : 0);
    }
  }
  const pairs: [number, number][] = [];
  let i = 0;
  let j = 0;
  while (i < removed.length && j < added.length) {
    const s = score(i, j);
    if (s > 0 && best[i][j] === s + best[i + 1][j + 1]) pairs.push([i++, j++]);
    else if (best[i][j] === best[i + 1][j]) i++;
    else j++;
  }
  return pairs;
}

/** Blocks between two similar pairs line up by position: a rewrite reads side by side. */
function pairByIndex(removed: string[], added: string[]): DiffBlock[] {
  return Array.from({ length: Math.max(removed.length, added.length) }, (_, i) => ({
    kind: i >= added.length ? "removed" : i >= removed.length ? "added" : "changed",
    before: removed[i] ?? null,
    after: added[i] ?? null,
  }));
}

/**
 * One replacement run (removed blocks, then added ones) in document order. Pairing the blocks
 * that resemble each other first keeps an inserted paragraph from taking the place of the
 * paragraph an edit came from.
 */
function pairRun(removed: string[], added: string[]): DiffBlock[] {
  const pairs =
    removed.length * added.length > MAX_PAIRING_CELLS ? [] : similarPairs(removed, added);
  const result: DiffBlock[] = [];
  let i = 0;
  let j = 0;
  for (const [pairedRemoved, pairedAdded] of [...pairs, [removed.length, added.length]]) {
    result.push(...pairByIndex(removed.slice(i, pairedRemoved), added.slice(j, pairedAdded)));
    if (pairedRemoved < removed.length) {
      result.push({ kind: "changed", before: removed[pairedRemoved], after: added[pairedAdded] });
    }
    i = pairedRemoved + 1;
    j = pairedAdded + 1;
  }
  return result;
}

function pairChanges(blocks: DiffBlock[]): DiffBlock[] {
  const result: DiffBlock[] = [];
  let i = 0;
  while (i < blocks.length) {
    if (blocks[i].kind !== "removed") {
      result.push(blocks[i++]);
      continue;
    }
    const removed: string[] = [];
    const added: string[] = [];
    for (; i < blocks.length && blocks[i].kind === "removed"; i++) removed.push(blocks[i].before!);
    for (; i < blocks.length && blocks[i].kind === "added"; i++) added.push(blocks[i].after!);
    result.push(...pairRun(removed, added));
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

/** The blocks whose keys match from the start (or, with `fromEnd`, the end) of both lists. */
function commonRun(before: Block[], after: Block[], limit: number, fromEnd: boolean): number {
  let run = 0;
  while (run < limit) {
    const b = fromEnd ? before[before.length - 1 - run] : before[run];
    const a = fromEnd ? after[after.length - 1 - run] : after[run];
    if (b.key !== a.key) break;
    run++;
  }
  return run;
}

function sameBlocks(before: Block[], after: Block[]): DiffBlock[] {
  return before.map((block, i) => ({ kind: "same", before: block.text, after: after[i].text }));
}

/**
 * The common start and end of both documents are unchanged, so only the middle is aligned: a
 * typical edit leaves the LCS a few blocks wide, and the size bound below applies to the middle.
 */
export function diffBlocks(before: string, after: string): DiffBlock[] {
  const beforeBlocks = blocksOf(before);
  const afterBlocks = blocksOf(after);
  const shortest = Math.min(beforeBlocks.length, afterBlocks.length);
  const head = commonRun(beforeBlocks, afterBlocks, shortest, false);
  const tail = commonRun(beforeBlocks, afterBlocks, shortest - head, true);
  const beforeMid = beforeBlocks.slice(head, beforeBlocks.length - tail);
  const afterMid = afterBlocks.slice(head, afterBlocks.length - tail);
  // Bound the quadratic LCS table and work for unusually large changes.
  const middle =
    beforeMid.length > 2_000 || afterMid.length > 2_000
      ? indexedDiff(beforeMid, afterMid)
      : pairChanges(alignBlocks(beforeMid, afterMid));
  return [
    ...sameBlocks(beforeBlocks.slice(0, head), afterBlocks.slice(0, head)),
    ...middle,
    ...sameBlocks(
      beforeBlocks.slice(beforeBlocks.length - tail),
      afterBlocks.slice(afterBlocks.length - tail),
    ),
  ];
}
