import { describe, expect, it } from "vitest";

import { diffBlocks, type DiffBlock } from "./block-diff";

const doc = (blocks: string[]): string => `${blocks.join("\n\n")}\n`;
const same = (text: string): DiffBlock => ({ kind: "same", before: text, after: text });
const added = (text: string): DiffBlock => ({ kind: "added", before: null, after: text });
const removed = (text: string): DiffBlock => ({ kind: "removed", before: text, after: null });
const changed = (before: string, after: string): DiffBlock => ({ kind: "changed", before, after });

describe("diffBlocks", () => {
  it("keeps identical documents same", () => {
    const blocks = ["# Heading", "First paragraph.", "Second paragraph."];
    expect(diffBlocks(doc(blocks), doc(blocks))).toEqual(blocks.map(same));
  });

  it("marks only an inserted paragraph added", () => {
    expect(
      diffBlocks(doc(["# Heading", "Original."]), doc(["# Heading", "Inserted.", "Original."])),
    ).toEqual([same("# Heading"), added("Inserted."), same("Original.")]);
  });

  it("marks a deleted heading removed", () => {
    expect(diffBlocks(doc(["# Heading", "Original."]), doc(["Original."]))).toEqual([
      removed("# Heading"),
      same("Original."),
    ]);
  });

  it("pairs a reworded paragraph with both source texts", () => {
    expect(
      diffBlocks(doc(["# Heading", "Old wording."]), doc(["# Heading", "New wording."])),
    ).toEqual([same("# Heading"), changed("Old wording.", "New wording.")]);
  });

  it("adds all blocks when before is empty", () => {
    const blocks = ["# Heading", "Paragraph."];
    expect(diffBlocks("", doc(blocks))).toEqual(blocks.map(added));
  });

  it("removes all blocks when after is empty", () => {
    const blocks = ["# Heading", "Paragraph."];
    expect(diffBlocks(doc(blocks), "")).toEqual(blocks.map(removed));
  });

  it("ignores whitespace-only differences while preserving both source slices", () => {
    const before = "  Some   words\non two lines.  ";
    const after = "Some words on two lines.";
    expect(diffBlocks(`\n${before}\n\n`, `\n\n${after}\n`)).toEqual([
      { kind: "same", before: before.trimStart(), after },
    ]);
  });

  it("treats a block moved from end to start as an addition and a removal", () => {
    expect(
      diffBlocks(doc(["First.", "Second.", "Moved."]), doc(["Moved.", "First.", "Second."])),
    ).toEqual([added("Moved."), same("First."), same("Second."), removed("Moved.")]);
  });

  it("preserves CRLF offsets and normalizes line endings for matching", () => {
    const before = "# Heading\r\n\r\nFirst line.\r\nSecond line.\r\n\r\nOld.\r\n";
    const after = "# Heading\n\nFirst line.\nSecond line.\n\nNew.\n";
    expect(diffBlocks(before, after)).toEqual([
      same("# Heading"),
      { kind: "same", before: "First line.\r\nSecond line.", after: "First line.\nSecond line." },
      changed("Old.", "New."),
    ]);
  });

  it("handles empty and whitespace-only documents", () => {
    expect(diffBlocks("", "")).toEqual([]);
    expect(diffBlocks(" \r\n\t\r\n", "\n\n")).toEqual([]);
  });

  it("uses complete top-level Markdown blocks and their original source", () => {
    const blocks = [
      "## Heading",
      "- First item\n- Second item",
      "> First quote line\n> Second quote line",
      "```ts\nconst value = 1;\n\nvalue;\n```",
      "| Name | Value |\n| --- | --- |\n| test | 1 |",
      "---",
      "[link]: https://example.com",
      "<div>\nRaw HTML.\n</div>",
    ];
    expect(diffBlocks(doc(blocks), doc(blocks))).toEqual(blocks.map(same));
  });

  it("pairs removed and added runs one to one, keeping trailing additions", () => {
    expect(
      diffBlocks(
        doc(["Start.", "Old one.", "Old two.", "End."]),
        doc(["Start.", "New one.", "New two.", "New three.", "End."]),
      ),
    ).toEqual([
      same("Start."),
      changed("Old one.", "New one."),
      changed("Old two.", "New two."),
      added("New three."),
      same("End."),
    ]);
  });

  it("keeps trailing removals when the removed run is longer", () => {
    expect(
      diffBlocks(
        doc(["Start.", "Old one.", "Old two.", "End."]),
        doc(["Start.", "New one.", "End."]),
      ),
    ).toEqual([same("Start."), changed("Old one.", "New one."), removed("Old two."), same("End.")]);
  });

  it("matches repeated blocks without losing occurrences", () => {
    expect(
      diffBlocks(doc(["Repeated.", "Unique.", "Repeated."]), doc(["Repeated.", "Repeated."])),
    ).toEqual([same("Repeated."), removed("Unique."), same("Repeated.")]);
  });

  it("uses LCS through 2,000 blocks", () => {
    const blocks = Array.from({ length: 2_000 }, (_, i) => `Paragraph ${i}.`);
    expect(diffBlocks(doc(blocks), doc(blocks))).toEqual(blocks.map(same));
  });

  it("falls back to index pairs above 2,000 blocks on either side", () => {
    const blocks = Array.from({ length: 2_001 }, (_, i) => `Paragraph ${i}.`);
    const first = blocks.slice(0, -1);
    const identical = first.map(same);
    expect(diffBlocks(doc(first), doc(blocks))).toEqual([...identical, added(blocks[2_000])]);
    expect(diffBlocks(doc(blocks), doc(first))).toEqual([...identical, removed(blocks[2_000])]);
  });

  it("marks only the edited pair changed in the index-pair fallback", () => {
    const blocks = Array.from({ length: 2_001 }, (_, i) => `Paragraph ${i}.`);
    const edited = blocks.map((text, i) => (i === 7 ? "Reworded." : text));
    const result = diffBlocks(doc(blocks), doc(edited));
    expect(result[7]).toEqual(changed("Paragraph 7.", "Reworded."));
    expect(result.filter((block) => block.kind !== "same")).toHaveLength(1);
  });

  it("aligns a document past the size bound by its middle: one edit gives one change", () => {
    const blocks = Array.from({ length: 3_000 }, (_, i) => `Paragraph ${i}.`);
    const edited = blocks.map((text, i) => (i === 1_500 ? "Reworded." : text));
    const result = diffBlocks(doc(blocks), doc(edited));
    expect(result).toHaveLength(3_000);
    expect(result.filter((block) => block.kind !== "same")).toEqual([
      changed("Paragraph 1500.", "Reworded."),
    ]);
    expect(result[1_500]).toEqual(changed("Paragraph 1500.", "Reworded."));
  });

  it("finds an insertion in the middle of a document past the size bound", () => {
    const blocks = Array.from({ length: 2_500 }, (_, i) => `Paragraph ${i}.`);
    const grown = [...blocks.slice(0, 1_200), "Inserted.", ...blocks.slice(1_200)];
    const result = diffBlocks(doc(blocks), doc(grown));
    expect(result.filter((block) => block.kind !== "same")).toEqual([added("Inserted.")]);
    expect(result).toHaveLength(2_501);
  });

  it("keeps an edit at the very start and at the very end", () => {
    const blocks = ["First.", "Middle.", "Last."];
    expect(diffBlocks(doc(blocks), doc(["Opening.", "Middle.", "Last."]))).toEqual([
      changed("First.", "Opening."),
      same("Middle."),
      same("Last."),
    ]);
    expect(diffBlocks(doc(blocks), doc(["First.", "Middle.", "Closing."]))).toEqual([
      same("First."),
      same("Middle."),
      changed("Last.", "Closing."),
    ]);
    expect(diffBlocks(doc(blocks), doc(["Zero.", ...blocks, "End."]))).toEqual([
      added("Zero."),
      ...blocks.map(same),
      added("End."),
    ]);
  });

  it("falls back to index pairs when the changed middle itself is past the size bound", () => {
    const old = Array.from({ length: 2_001 }, (_, i) => `Old ${i}.`);
    const next = Array.from({ length: 2_001 }, (_, i) => `New ${i}.`);
    const result = diffBlocks(doc(["Top.", ...old, "Bottom."]), doc(["Top.", ...next, "Bottom."]));
    expect(result).toHaveLength(2_003);
    expect(result[0]).toEqual(same("Top."));
    expect(result[1]).toEqual(changed("Old 0.", "New 0."));
    expect(result[2_002]).toEqual(same("Bottom."));
  });

  it("keeps a re-indented line in a fenced code block changed", () => {
    const fence = (indent: string): string => `\`\`\`yaml\nstage:\n${indent}id: a\n\`\`\``;
    const result = diffBlocks(doc([fence("  ")]), doc([fence("    ")]));
    expect(result).toEqual([changed(fence("  "), fence("    "))]);
  });

  it("keeps a nested list changed when it is flattened", () => {
    const nested = "- a\n  - b";
    const flat = "- a\n- b";
    expect(diffBlocks(doc([nested]), doc([flat]))).toEqual([changed(nested, flat)]);
  });

  it("ignores trailing blanks and line endings in a code block", () => {
    const before = "```text\nline  \nnext\n```\n";
    const after = "```text\r\nline\r\nnext\r\n```\r\n";
    expect(diffBlocks(before, after).map((block) => block.kind)).toEqual(["same"]);
  });
});

describe("diffBlocks pairing of a replaced run", () => {
  const intro = "The intro stays exactly as it was.";
  const outro = "The outro stays exactly as it was.";
  const edited = (word: string): string =>
    `The ${word} overview is written once a plan has settled and every stage has an owner.`;
  const inserted = "Plan authoring gate: no plan is written before its metaplan is settled.";

  it("pairs an edited paragraph with its original and marks the paragraph before it added", () => {
    expect(
      diffBlocks(doc([intro, edited("engineering")]), doc([intro, inserted, edited("metaplan")])),
    ).toEqual([same(intro), added(inserted), changed(edited("engineering"), edited("metaplan"))]);
  });

  it("pairs an edited paragraph with its original and marks the paragraph before it removed", () => {
    expect(
      diffBlocks(doc([intro, inserted, edited("engineering")]), doc([intro, edited("metaplan")])),
    ).toEqual([same(intro), removed(inserted), changed(edited("engineering"), edited("metaplan"))]);
  });

  it("keeps an insert between two edited paragraphs added", () => {
    const first = edited("first");
    const second = "A second paragraph names the owner of each stage in the plan, one by one.";
    expect(
      diffBlocks(
        doc([intro, first, second, outro]),
        doc([intro, edited("revised first"), inserted, `${second} Done.`, outro]),
      ),
    ).toEqual([
      same(intro),
      changed(first, edited("revised first")),
      added(inserted),
      changed(second, `${second} Done.`),
      same(outro),
    ]);
  });

  it("pairs a dissimilar block between two similar pairs by position", () => {
    const middle = ["Sunrise over cold water.", "Granite walls and nothing else."];
    expect(
      diffBlocks(
        doc([intro, edited("old"), middle[0], edited("older"), outro]),
        doc([intro, edited("new"), middle[1], edited("newer"), outro]),
      ),
    ).toEqual([
      same(intro),
      changed(edited("old"), edited("new")),
      changed(middle[0], middle[1]),
      changed(edited("older"), edited("newer")),
      same(outro),
    ]);
  });

  it("still pairs a dissimilar one-to-one replacement side by side", () => {
    expect(
      diffBlocks(
        doc([intro, "One two three.", "Four five six.", outro]),
        doc([intro, "Red green blue.", "Cyan magenta yellow.", outro]),
      ),
    ).toEqual([
      same(intro),
      changed("One two three.", "Red green blue."),
      changed("Four five six.", "Cyan magenta yellow."),
      same(outro),
    ]);
  });

  it("pairs by index when the run is past the scoring bound", () => {
    const filler = (prefix: string): string[] =>
      Array.from({ length: 100 }, (_, i) => `${prefix} ${i}.`);
    const result = diffBlocks(
      doc([intro, edited("engineering"), ...filler("Old"), outro]),
      doc([intro, inserted, edited("metaplan"), ...filler("New"), outro]),
    );
    // Scored, the edited paragraph would pair with its edit; by index it pairs with the insert.
    expect(result[1]).toEqual(changed(edited("engineering"), inserted));
    expect(result[2]).toEqual(changed("Old 0.", edited("metaplan")));
    expect(result).toHaveLength(104);
  });
});
