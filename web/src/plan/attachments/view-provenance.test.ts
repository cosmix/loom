import { afterEach, describe, expect, it } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import { renderedViewProvenance } from "./view-provenance";

const sections: SectionView[] = [
  { level: 2, title: "First", anchor: "first", start_line: 1, end_line: 3 },
  { level: 2, title: "Second", anchor: "second", start_line: 4, end_line: 6 },
];

function select(start: Node, startOffset: number, end: Node, endOffset: number): Selection {
  const range = document.createRange();
  range.setStart(start, startOffset);
  range.setEnd(end, endOffset);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  return selection;
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.replaceChildren();
});

describe("rendered view provenance", () => {
  it("finds the nearest section anchor and preserves the visible selection", () => {
    document.body.innerHTML =
      '<h2 id="first">First</h2><p>Alpha</p><div><h2 id="second">Second</h2><p id="unrelated">Beta words</p></div>';
    const text = document.querySelector("#unrelated")!.firstChild!;
    expect(renderedViewProvenance("doc/notes.md", sections)(select(text, 0, text, 4))).toEqual({
      kind: "source",
      path: "doc/notes.md",
      anchor: "second",
      text: "Beta",
      lines: null,
      stageId: null,
      hash: null,
    });
  });

  it("uses the selection start for the anchor and stage row", () => {
    document.body.innerHTML =
      '<h2 id="first">First</h2><div id="stage:build"><span>Build</span></div><h2 id="second">Second</h2><p>Next</p>';
    const start = document.querySelector("span")!.firstChild!;
    const end = document.querySelector("p")!.firstChild!;
    expect(renderedViewProvenance("doc/plan.md", sections)(select(start, 0, end, 4))).toMatchObject(
      {
        anchor: "first",
        stageId: "build",
      },
    );
  });

  it("handles selections before the first heading and collapsed or empty selections", () => {
    document.body.innerHTML = '<p>Intro</p><h2 id="first">First</h2>';
    const text = document.querySelector("p")!.firstChild!;
    const provenance = renderedViewProvenance("doc/notes.md", sections);
    expect(provenance(select(text, 0, text, 5))).toMatchObject({ anchor: null, stageId: null });
    expect(provenance(select(text, 0, text, 0))).toBeNull();
    window.getSelection()!.removeAllRanges();
    expect(provenance(window.getSelection()!)).toBeNull();
  });
});
