# W1: block diff model (codex sol, high)

Read `common.md` beside this file first. It pins the type and the matching rule.

## Files you own

- `web/src/plan/diff/block-diff.ts` (new)
- `web/src/plan/diff/block-diff.test.ts` (new)

## Steps

1. Import `parseMarkdown` from `@/plan/markdown/markdown`. Confirm its export and return type with
   `loom map --outline web/src/plan/markdown/markdown.tsx`.
2. `blocksOf(source)`: for each node in `root.children` that has a `position`, slice
   `source.slice(start.offset, end.offset)`. Nodes without a position are skipped.
3. Compute the LCS over normalized keys. Use an O(n·m) table, which is fine for documents of a few
   hundred blocks. Above 2,000 blocks on either side, fall back to one `changed` block per index
   pair, plus the trailing added or removed blocks, and comment why.
4. Walk the alignment into `DiffBlock[]`. Pair adjacent removed-then-added runs into `changed`,
   one to one.
5. Tests:
   - identical documents give all `same`;
   - an inserted paragraph gives exactly one `added`;
   - a deleted heading gives one `removed`;
   - a reworded paragraph gives one `changed` with both sides;
   - an empty `before` gives all `added`;
   - an empty `after` gives all `removed`;
   - a whitespace-only change gives `same`;
   - a block moved from the end to the start gives one added and one removed, never a changed
     cascade;
   - a CRLF source works.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean. Run `node_modules/.bin/vitest run
src/plan/diff/block-diff.test.ts` if vitest runs for you. Report the exported names.
