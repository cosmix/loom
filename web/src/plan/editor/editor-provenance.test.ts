import { afterEach, describe, expect, it } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import { editorProvenance } from "./editor-provenance";

const sections: SectionView[] = [
  { level: 2, title: "Parent", anchor: "parent", start_line: 3, end_line: 12 },
  { level: 3, title: "Child", anchor: "child", start_line: 5, end_line: 8 },
];
const stages = [{ id: "build", start_line: 6, end_line: 7 }];

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

describe("editor provenance", () => {
  it("uses the deepest containing section and stage at the selection start", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    expect(provenance(select("selected source"), { start: 6, end: 10 })).toEqual({
      kind: "source",
      path: "doc/plan.md",
      anchor: "child",
      text: "selected source",
      lines: { start: 6, end: 10 },
      stageId: "build",
      hash: null,
    });
  });

  it("respects inclusive span boundaries and selections outside spans", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    const selection = select("text");
    expect(provenance(selection, { start: 7, end: 7 })).toMatchObject({ stageId: "build" });
    expect(provenance(selection, { start: 8, end: 8 })).toMatchObject({
      anchor: "child",
      stageId: null,
    });
    expect(provenance(selection, { start: 2, end: 3 })).toMatchObject({
      anchor: null,
      stageId: null,
    });
    expect(provenance(selection, { start: 10, end: 10 })).toMatchObject({ anchor: "parent" });
  });

  it("rejects empty selections and invalid line ranges", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    expect(provenance(select("text", 0, 0), { start: 1, end: 1 })).toBeNull();
    const selection = select("text");
    expect(provenance(selection, { start: 0, end: 1 })).toBeNull();
    expect(provenance(selection, { start: 2, end: 1 })).toBeNull();
    expect(provenance(selection, { start: 1.5, end: 2 })).toBeNull();
  });

  it("prefers the editor's source text over the DOM text of the selection", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    const lines = { start: 6, end: 6 };
    expect(provenance(select("some bold text"), lines, "some **bold** text")).toMatchObject({
      text: "some **bold** text",
    });
  });

  it("falls back to the DOM text when the source text is null, undefined or empty", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    const lines = { start: 6, end: 6 };
    for (const sourceText of [null, undefined, ""]) {
      expect(provenance(select("dom text"), lines, sourceText)).toMatchObject({ text: "dom text" });
    }
  });

  it("still rejects a collapsed selection when source text is given", () => {
    const provenance = editorProvenance("doc/plan.md", sections, stages);
    expect(provenance(select("text", 0, 0), { start: 1, end: 1 }, "**text**")).toBeNull();
  });
});
