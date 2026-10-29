import { afterEach, describe, expect, it } from "vitest";

import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import {
  attachmentFromSelection,
  attachmentsFromSelection,
  type SentenceResolver,
} from "./selection";

const page: ConceptPage = {
  feature: "demo",
  key: "demo/product/section_05",
  title: "Concept",
  path: "doc/product/section_05.md",
  file_hash: "summary-hash",
  colour: null,
  state: null,
  plan_state: null,
  last_pressure: null,
  key_chips: [],
  unlisted: [],
  implements: null,
  implemented_by: [],
  empty: null,
  building: null,
  estimate: null,
  blocks: [
    {
      kind: "paragraph",
      sentences: [
        {
          id: "s1",
          text: "Summary words",
          marks: [],
          citations: [],
          rests_on: [{ path: "doc/product/source.md", anchor: "details" }],
          plans: [],
        },
      ],
    },
  ],
};

const own =
  page.blocks[0]!.kind === "paragraph"
    ? (page.blocks[0] as { sentences: Sentence[] }).sentences[0]!
    : (undefined as never);
const resolveOwn: SentenceResolver = (summary, id) =>
  summary === "demo/product/section_05" && id === own.id
    ? { sentence: own, hash: page.file_hash }
    : undefined;

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

describe("concept selection provenance", () => {
  it("captures a summary sentence and its sources", () => {
    document.body.innerHTML =
      '<span data-summary="demo/product/section_05" data-sentence-id="s1">Summary words</span>';
    const text = document.querySelector("span")!.firstChild!;
    expect(attachmentFromSelection(select(text, 0, text, 7), resolveOwn)).toEqual({
      kind: "summary",
      summary: "demo/product/section_05",
      sentence: "s1",
      text: "Summary",
      restsOn: [{ path: "doc/product/source.md", anchor: "details" }],
      hash: "summary-hash",
    });
  });

  it("captures a passage hash and the selected block line", () => {
    document.body.innerHTML =
      '<div data-passage="doc/product/source.md#details" data-hash="source-hash" data-start-line="3" data-end-line="8"><p data-line="5">Source words</p></div>';
    const text = document.querySelector("p")!.firstChild!;
    expect(attachmentFromSelection(select(text, 0, text, 6), resolveOwn)).toEqual({
      kind: "source",
      path: "doc/product/source.md",
      anchor: "details",
      text: "Source",
      lines: { start: 5, end: 5 },
      stageId: null,
      hash: "source-hash",
    });
  });

  it("selection inside an opened cited run uses the cited summary", () => {
    document.body.innerHTML =
      '<span data-summary="demo/product/section_02" data-sentence-id="c9">Cited words</span>';
    const text = document.querySelector("span")!.firstChild!;
    const cited = { ...own, id: "c9", rests_on: [{ path: "doc/b.md", anchor: "x" }] };
    const resolve: SentenceResolver = (summary, id) =>
      summary === "demo/product/section_02" && id === "c9"
        ? { sentence: cited, hash: "cited-hash" }
        : undefined;
    expect(attachmentFromSelection(select(text, 0, text, 5), resolve)).toEqual({
      kind: "summary",
      summary: "demo/product/section_02",
      sentence: "c9",
      text: "Cited",
      restsOn: [{ path: "doc/b.md", anchor: "x" }],
      hash: "cited-hash",
    });
  });

  it("colliding sentence ids resolve by summary", () => {
    document.body.innerHTML =
      '<span data-summary="demo/product/section_02" data-sentence-id="s1">Other words</span>';
    const text = document.querySelector("span")!.firstChild!;
    const other = { ...own, rests_on: [{ path: "doc/other.md", anchor: "y" }] };
    const resolve: SentenceResolver = (summary) =>
      summary === "demo/product/section_02"
        ? { sentence: other, hash: "other-hash" }
        : { sentence: own, hash: "summary-hash" };
    const draft = attachmentFromSelection(select(text, 0, text, 5), resolve);
    expect(draft).toMatchObject({ restsOn: other.rests_on, hash: "other-hash" });
  });

  it("drops the draft when the summary does not resolve", () => {
    document.body.innerHTML =
      '<span data-summary="demo/product/section_09" data-sentence-id="s1">Gone words</span>';
    const text = document.querySelector("span")!.firstChild!;
    expect(attachmentFromSelection(select(text, 0, text, 4), () => undefined)).toBeNull();
  });
});

describe("selection spanning several parts", () => {
  const sentenceHtml = (id: string, text: string) =>
    `<span data-summary="demo/product/section_05" data-sentence-id="${id}">${text}</span>`;
  const resolveAny: SentenceResolver = (summary, id) =>
    summary === "demo/product/section_05" && id !== "gone"
      ? {
          sentence: { ...own, id, rests_on: [{ path: `doc/product/${id}.md`, anchor: id }] },
          hash: "summary-hash",
        }
      : undefined;
  const textOf = (selector: string) => document.querySelector(selector)!.firstChild!;
  const sentenceText = (id: string) => textOf(`[data-sentence-id="${id}"]`);
  const sentenceIds = (drafts: ReturnType<typeof attachmentsFromSelection>) =>
    drafts.map((draft) => (draft.kind === "summary" ? draft.sentence : null));

  it("carries one draft per selected sentence, in reading order", () => {
    document.body.innerHTML =
      `<p>${sentenceHtml("s1", "First words")}${sentenceHtml("s2", "Second words")}</p>` +
      `<p>${sentenceHtml("s3", "Third words")}</p>`;
    const sel = select(sentenceText("s1"), 6, sentenceText("s3"), 5);
    const drafts = attachmentsFromSelection(sel, resolveAny);
    expect(drafts.map((draft) => draft.text)).toEqual(["words", "Second words", "Third"]);
    expect(sentenceIds(drafts)).toEqual(["s1", "s2", "s3"]);
    expect(drafts[1]).toMatchObject({ restsOn: [{ path: "doc/product/s2.md", anchor: "s2" }] });
    expect(attachmentFromSelection(sel, resolveAny)).toEqual(drafts[0]);
  });

  it("carries a summary sentence and a passage, each with its own provenance", () => {
    document.body.innerHTML =
      sentenceHtml("s1", "Summary words") +
      '<div data-passage="doc/product/source.md#details" data-hash="source-hash" data-start-line="3" data-end-line="8"><p data-line="5">Source words</p></div>';
    const sel = select(textOf("span"), 0, textOf("p"), 6);
    expect(attachmentsFromSelection(sel, resolveAny)).toEqual([
      expect.objectContaining({ kind: "summary", sentence: "s1", text: "Summary words" }),
      expect.objectContaining({
        kind: "source",
        anchor: "details",
        text: "Source",
        hash: "source-hash",
      }),
    ]);
  });

  it("drops a part that does not resolve and keeps the others", () => {
    document.body.innerHTML = sentenceHtml("gone", "Lost") + sentenceHtml("s2", "Kept words");
    const sel = select(sentenceText("gone"), 0, sentenceText("s2"), 4);
    const drafts = attachmentsFromSelection(sel, resolveAny);
    expect(drafts.map((draft) => draft.text)).toEqual(["Kept"]);
    expect(attachmentFromSelection(sel, resolveAny)).toEqual(drafts[0]);
  });

  it("returns nothing for a collapsed selection", () => {
    document.body.innerHTML = sentenceHtml("s1", "First words");
    const sel = select(sentenceText("s1"), 2, sentenceText("s1"), 2);
    expect(attachmentsFromSelection(sel, resolveAny)).toEqual([]);
  });
});
