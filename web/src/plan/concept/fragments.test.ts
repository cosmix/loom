import { describe, expect, it } from "vitest";

import type { ConceptPage, Mark, Sentence } from "@/plan/api/schema-concept";

import {
  columnPage,
  fragmentMembers,
  mergeFragments,
  openKeys,
  runAt,
  runSentences,
} from "./fragments";
import type { CitedLookup } from "./open-state";
import type { OpenRun } from "./types";

const mark = (doc: string): Mark => ({
  doc: { id: doc, title: doc, path: `doc/${doc}.md`, page: `demo/${doc}` },
  colour: 1,
  out_of_date: null,
});

/** A sentence citing the sections `anchors` of doc/a.md. */
function source(id: string, anchors: string[], marks: Mark[] = []): Sentence {
  return {
    id,
    text: `Text ${id}.`,
    marks,
    citations: anchors.map((anchor) => ({ kind: "source" as const, path: "doc/a.md", anchor })),
    rests_on: anchors.map((anchor) => ({ path: "doc/a.md", anchor })),
    plans: [],
  };
}

/** A sentence citing the sentences `ids` of the summary `summary`. */
function citing(id: string, ids: string[], summary = "s"): Sentence {
  return {
    ...source(id, []),
    citations: ids.map((target) => ({ kind: "sentence" as const, summary, id: target })),
  };
}

const ids = (sentences: Sentence[]) => sentences.map((sentence) => sentence.id);

function pageOf(sentences: Sentence[]): ConceptPage {
  return {
    feature: "demo",
    key: "demo/product/section_01",
    title: "Planning",
    path: null,
    file_hash: null,
    colour: null,
    state: null,
    plan_state: null,
    last_pressure: null,
    key_chips: [],
    blocks: [
      { kind: "heading", level: 1, text: "Planning" },
      { kind: "paragraph", sentences },
    ],
    unlisted: [],
    implements: null,
    implemented_by: [],
    empty: null,
    building: null,
    estimate: null,
  };
}

describe("openKeys", () => {
  it("names the cited sentences, else the source sections", () => {
    expect(openKeys(citing("a", ["x", "y"]))).toEqual(["s|s#x", "s|s#y"]);
    expect(openKeys(source("a", ["one", "two"]))).toEqual(["p|doc/a.md#one", "p|doc/a.md#two"]);
  });

  it("is empty for a sentence with no citations, even one that rests on a section", () => {
    expect(openKeys({ ...source("a", ["one"]), citations: [] })).toEqual([]);
  });
});

describe("mergeFragments", () => {
  it("joins a sentence whose keys meet the fragment's gathered keys", () => {
    const merged = mergeFragments([
      source("a", ["a"]),
      source("b", ["a", "b"]),
      source("c", ["b"]),
    ]);
    expect(ids(merged)).toEqual(["a"]);
    expect(ids(fragmentMembers(merged[0]!))).toEqual(["a", "b", "c"]);
  });

  it("starts a new fragment when a later sentence only meets an earlier one", () => {
    const merged = mergeFragments([source("a", ["a"]), source("b", ["b"]), source("c", ["a"])]);
    expect(ids(merged)).toEqual(["a", "b", "c"]);
  });

  it("merges sentence-citing sentences that share a cited sentence", () => {
    const merged = mergeFragments([
      citing("a", ["x"]),
      citing("b", ["x", "y"]),
      citing("c", ["z"]),
    ]);
    expect(ids(merged)).toEqual(["a", "c"]);
    expect(merged[0]!.citations.map((citation) => citation.kind)).toEqual(["sentence", "sentence"]);
  });

  it("keeps a sentence with no keys between two sharing ones apart from both", () => {
    const plain = { ...source("p", []), citations: [] };
    const merged = mergeFragments([source("a", ["a"]), plain, source("c", ["a"])]);
    expect(ids(merged)).toEqual(["a", "p", "c"]);
  });

  it("builds the lead's id, the joined text and the unions in first-seen order", () => {
    const [merged] = mergeFragments([
      source("a", ["one"], [mark("x")]),
      source("b", ["one", "two"], [mark("y"), mark("x")]),
    ]);
    expect(merged!.id).toBe("a");
    expect(merged!.text).toBe("Text a. Text b.");
    expect(merged!.marks.map((item) => item.doc.id)).toEqual(["x", "y"]);
    expect(merged!.citations).toEqual([
      { kind: "source", path: "doc/a.md", anchor: "one" },
      { kind: "source", path: "doc/a.md", anchor: "two" },
    ]);
    expect(merged!.rests_on).toEqual([
      { path: "doc/a.md", anchor: "one" },
      { path: "doc/a.md", anchor: "two" },
    ]);
  });

  it("returns a lone sentence as it is, and it stands for itself", () => {
    const only = source("a", ["one"]);
    const [same] = mergeFragments([only]);
    expect(same).toBe(only);
    expect(fragmentMembers(only)).toEqual([only]);
  });
});

describe("columnPage", () => {
  it("merges each paragraph, keeps headings and detail, and returns one object per page", () => {
    const page = {
      ...pageOf([source("a", ["one"]), source("b", ["one"])]),
      detail: [source("d", ["one"])],
    };
    const view = columnPage(page);
    expect(columnPage(page)).toBe(view);
    expect(view.blocks[0]).toBe(page.blocks[0]);
    const paragraph = view.blocks[1]!;
    expect(paragraph.kind === "paragraph" && ids(paragraph.sentences)).toEqual(["a"]);
    expect(view.detail).toBe(page.detail);
  });
});

describe("runSentences", () => {
  const cited: Record<string, Sentence> = {
    x: source("x", ["one"]),
    y: source("y", ["one", "two"]),
    z: source("z", ["three"]),
  };
  const lookup: CitedLookup = (_summary, id) => cited[id];

  it("merges adjacent resolved entries and keeps the lead's citation", () => {
    const entries = runSentences(citing("o", ["x", "y", "z"]), lookup);
    expect(entries.map((entry) => entry.citation.id)).toEqual(["x", "z"]);
    expect(entries.map((entry) => entry.sentence?.text)).toEqual(["Text x. Text y.", "Text z."]);
  });

  it("leaves an unresolved entry alone and merges only across resolved ones", () => {
    const entries = runSentences(citing("o", ["x", "gone", "y"]), lookup);
    expect(entries.map((entry) => entry.citation.id)).toEqual(["x", "gone", "y"]);
    expect(entries.map((entry) => entry.sentence?.id)).toEqual(["x", undefined, "y"]);
  });
});

describe("runAt", () => {
  const run = (sentenceId: string, parent: string | null = null): OpenRun => ({
    level: 0,
    sentenceId,
    restsOn: [],
    parent,
  });
  const siteOf = (sentence: Sentence, parent: string | null = null) => ({
    sentence,
    summary: "s",
    parent,
    marks: [],
  });

  it("finds the run opened at any member of a fragment, at the same parent", () => {
    const [merged] = mergeFragments([source("a", ["one"]), source("b", ["one"])]);
    expect(runAt([run("b")], siteOf(merged!))?.sentenceId).toBe("b");
    expect(runAt([run("b", "o")], siteOf(merged!))).toBeUndefined();
    expect(runAt([run("c")], siteOf(merged!))).toBeUndefined();
  });
});
