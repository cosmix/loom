import { afterEach, describe, expect, it } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import { diffProvenance } from "./diff-provenance";

const sections: SectionView[] = [
  { level: 2, title: "Parent", anchor: "parent", start_line: 3, end_line: 12 },
  { level: 3, title: "Child", anchor: "child", start_line: 5, end_line: 8 },
];

function select(text: string, start = 0, end = text.length): Selection {
  const node = document.createTextNode(text);
  document.body.append(node);
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  return selection;
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.replaceChildren();
});

describe("diff provenance", () => {
  it("captures the selected source and the section at the first line", () => {
    const provenance = diffProvenance("doc/plan.md", sections);
    expect(provenance(select("before\nafter"), { start: 6, end: 10 })).toEqual({
      kind: "source",
      path: "doc/plan.md",
      anchor: "child",
      text: "before\nafter",
      lines: { start: 6, end: 10 },
      stageId: null,
      hash: null,
    });
  });

  it("uses inclusive section boundaries and leaves the anchor null outside sections", () => {
    const provenance = diffProvenance("doc/plan.md", sections);
    const selection = select("text");
    expect(provenance(selection, { start: 8, end: 8 })).toMatchObject({ anchor: "child" });
    expect(provenance(selection, { start: 9, end: 9 })).toMatchObject({ anchor: "parent" });
    expect(provenance(selection, { start: 2, end: 3 })).toMatchObject({ anchor: null });
  });

  it("rejects empty selections and invalid line ranges", () => {
    const provenance = diffProvenance("doc/plan.md", sections);
    expect(provenance(select("text", 0, 0), { start: 1, end: 1 })).toBeNull();
    const selection = select("text");
    expect(provenance(selection, { start: 0, end: 1 })).toBeNull();
    expect(provenance(selection, { start: 2, end: 1 })).toBeNull();
    expect(provenance(selection, { start: 1.5, end: 2 })).toBeNull();
  });
});

describe("diff provenance sides", () => {
  it("anchors the after side to the current sections and leaves the before side unanchored", () => {
    const provenance = diffProvenance("doc/plan.md", sections);
    const selection = select("text");
    const draft = (side?: "before" | "after") => ({
      kind: "source",
      path: "doc/plan.md",
      anchor: side === "before" ? null : "child",
      text: "text",
      lines: { start: 6, end: 6 },
      stageId: null,
      hash: null,
    });
    expect(provenance(selection, { start: 6, end: 6, side: "after" })).toEqual(draft("after"));
    expect(provenance(selection, { start: 6, end: 6 })).toEqual(draft());
    expect(provenance(selection, { start: 6, end: 6, side: "before" })).toEqual(draft("before"));
  });
});
