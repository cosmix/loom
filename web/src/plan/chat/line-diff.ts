/** Lines added and removed between two texts, without CodeMirror (see the lazy boundary). */
export type LineCounts = { added: number; removed: number };

/**
 * Edit distance past which the exact count is abandoned. Myers needs O(D^2) steps, so a rewritten
 * document of tens of thousands of lines would hang the tab; 2000 edits cost a few million steps.
 * Revisit if footers need exact counts for rewrites that large.
 */
const MAX_EDIT_DISTANCE = 2000;

/**
 * Length of the shortest edit script between two line lists (Myers, O((n+m)·D) time, O(n+m)
 * memory), so a document of thousands of lines with a few edits stays cheap. Returns null when
 * the distance exceeds `MAX_EDIT_DISTANCE`.
 */
function editDistance(a: string[], b: string[]): number | null {
  const n = a.length;
  const m = b.length;
  const max = n + m;
  if (max === 0) return 0;
  const reach = new Int32Array(2 * max + 2);
  for (let d = 0; d <= Math.min(max, MAX_EDIT_DISTANCE); d++) {
    for (let k = -d; k <= d; k += 2) {
      const down = k === -d || (k !== d && reach[max + k - 1]! < reach[max + k + 1]!);
      let x = down ? reach[max + k + 1]! : reach[max + k - 1]! + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      reach[max + k] = x;
      if (x >= n && y >= m) return d;
    }
  }
  return null;
}

/** An empty text has no lines, and a final newline ends the last line rather than adding one. */
function toLines(text: string): string[] {
  const lines = text.split("\n");
  if (lines.at(-1) === "") lines.pop();
  return lines;
}

export function lineCounts(before: string, after: string): LineCounts {
  const a = toLines(before);
  const b = toLines(after);
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const middleA = a.slice(start, endA);
  const middleB = b.slice(start, endB);
  const distance = editDistance(middleA, middleB);
  // Beyond the cap the count is approximate: every line between the common prefix and suffix is
  // treated as removed and re-added.
  if (distance === null) return { added: middleB.length, removed: middleA.length };
  const common = (middleA.length + middleB.length - distance) / 2;
  return { added: middleB.length - common, removed: middleA.length - common };
}
