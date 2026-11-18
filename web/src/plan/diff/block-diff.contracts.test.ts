import { expect, it } from "vitest";

// A namespace import keeps the file loadable before `diffBlocks` exists, so the failure happens
// inside the test.
import * as blockDiff from "./block-diff";

const HEADING = "# Release notes";
const FIRST = "The first paragraph stays as it was.";
const SECOND = "The second paragraph stays as it was.";
const THIRD = "The third paragraph stays as it was.";
const LAST_BEFORE = "The last paragraph says one thing.";
const LAST_AFTER = "The last paragraph now says another thing.";
const INSERTED = "A new paragraph goes straight after the heading.";

const doc = (blocks: string[]): string => `${blocks.join("\n\n")}\n`;

it("diffBlocks marks an inserted paragraph as added and keeps the blocks after it same", () => {
  const before = doc([HEADING, FIRST, SECOND, THIRD, LAST_BEFORE]);
  const after = doc([HEADING, INSERTED, FIRST, SECOND, THIRD, LAST_AFTER]);

  // A positional comparison of block i with block i would mark every block after the insertion
  // as changed; the matched diff marks only the insertion and the reworded paragraph.
  expect(blockDiff.diffBlocks(before, after)).toEqual([
    { kind: "same", before: HEADING, after: HEADING },
    { kind: "added", before: null, after: INSERTED },
    { kind: "same", before: FIRST, after: FIRST },
    { kind: "same", before: SECOND, after: SECOND },
    { kind: "same", before: THIRD, after: THIRD },
    { kind: "changed", before: LAST_BEFORE, after: LAST_AFTER },
  ]);
});
