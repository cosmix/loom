import { describe, expect, it } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema, type ConceptPage, type Sentence } from "@/plan/api/schema-concept";

import { carryOpenState, carryStoredRuns, syntheticPrevious, type OpenRun } from "./open-state";
import type { SourceAnchorRef } from "./types";

const FILE = "doc/product/section_01.md";
const at = (anchor: string, title?: string): SourceAnchorRef =>
  title === undefined ? { path: FILE, anchor } : { path: FILE, anchor, title };
const bare = (anchors: SourceAnchorRef[]) => anchors.map(({ path, anchor }) => ({ path, anchor }));

function sentence(id: string, anchors: SourceAnchorRef[], cites: string[] = []): Sentence {
  const sources = bare(anchors).map((anchor) => ({ kind: "source" as const, ...anchor }));
  const sentences = cites.map((cited) => ({
    kind: "sentence" as const,
    summary: "product/section_01",
    id: cited,
  }));
  return {
    id,
    text: `Sentence ${id}.`,
    marks: [],
    citations: [...sentences, ...sources],
    rests_on: bare(anchors),
    plans: [],
  };
}

function page(...paragraphs: Sentence[][]): ConceptPage {
  const base = conceptPageSchema.parse(structuredClone(summaryFixture));
  const blocks = paragraphs.map((sentences) => ({ kind: "paragraph" as const, sentences }));
  return { ...base, blocks: [{ kind: "heading", level: 1, text: "Planning" }, ...blocks] };
}

const run = (
  sentenceId: string,
  restsOn: SourceAnchorRef[],
  parent: string | null = null,
): OpenRun => ({
  level: 1,
  sentenceId,
  restsOn,
  parent,
});

const passage = (anchor: SourceAnchorRef, parent: string): OpenRun => ({
  level: 2,
  sentenceId: `${anchor.path}#${anchor.anchor}`,
  restsOn: [anchor],
  parent,
});

describe("carryOpenState", () => {
  it("kept unchanged", () => {
    const current = page([sentence("s0", [at("a"), at("b")]), sentence("s1", [at("a"), at("b")])]);
    const open = [run("s1", [at("a", "Alpha"), at("b")]), passage(at("b", "Bee"), "s1")];
    const result = carryOpenState(current, current, open);
    expect(result.open).toEqual(open);
    expect(result.notes).toEqual([]);
  });

  it("moved to best overlap", () => {
    const prev = page([sentence("s1", [at("a"), at("b"), at("c")])]);
    const next = page([sentence("s7", [at("a")]), sentence("s8", [at("b"), at("c")])]);
    const result = carryOpenState(prev, next, [run("s1", [at("a"), at("b", "Bee"), at("c")])]);
    expect(result.open).toEqual([run("s8", [at("b", "Bee"), at("c")])]);
    expect(result.notes).toEqual([]);
  });

  it("tie by reading order", () => {
    const prev = page([sentence("s1", [at("a"), at("b")])]);
    const next = page(
      [sentence("s4", [at("z")])],
      [sentence("s5", [at("b")]), sentence("s6", [at("a")])],
    );
    const result = carryOpenState(prev, next, [run("s1", [at("a"), at("b")])]);
    expect(result.open.map((entry) => entry.sentenceId)).toEqual(["s5"]);
  });

  it("nested run dropped with its parent", () => {
    const prev = page([sentence("o1", [at("a", "Alpha")], ["x1"])]);
    const next = page([sentence("o9", [at("z")])]);
    const open: OpenRun[] = [
      { level: 0, sentenceId: "o1", restsOn: [at("a", "Alpha")], parent: null },
      run("x1", [at("a", "Alpha")], "o1"),
      passage(at("a", "Alpha"), "x1"),
    ];
    const result = carryOpenState(prev, next, open);
    expect(result.open).toEqual([]);
    expect(result.notes).toEqual([
      {
        paragraphOf: "",
        text: "This summary was rewritten and no longer draws on Alpha",
        path: FILE,
      },
    ]);
  });

  it("passage deletion note", () => {
    const prev = page([sentence("s1", [at("a"), at("b")])]);
    const next = page([sentence("s2", [at("a")])]);
    const open = [run("s1", [at("a"), at("b")]), passage(at("b", "Bee"), "s1")];
    const result = carryOpenState(prev, next, open);
    expect(result.open).toEqual([run("s2", [at("a")])]);
    expect(result.notes).toEqual([
      { paragraphOf: "s2", text: "Section Bee was removed from section_01.md", path: FILE },
    ]);
  });

  it("rewritten summary note text", () => {
    const prev = page([sentence("s0", [at("y")]), sentence("s1", [at("a", "Alpha"), at("b")])]);
    const next = page([sentence("s0", [at("y")]), sentence("s2", [at("z")])]);
    const result = carryOpenState(prev, next, [run("s1", [at("a", "Alpha"), at("b", "Bee")])]);
    expect(result.open).toEqual([]);
    expect(result.notes).toEqual([
      {
        paragraphOf: "s0",
        text: "This summary was rewritten and no longer draws on Alpha",
        path: FILE,
      },
    ]);
  });

  it("renamed heading", () => {
    const prev = page([sentence("s1", [at("port"), at("other")])]);
    const next = page([sentence("s1", [at("ports"), at("other")])]);
    const open = [run("s1", [at("port"), at("other")]), passage(at("port", "Port"), "s1")];
    const result = carryOpenState(prev, next, open);
    expect(result.open).toEqual([run("s1", [at("ports"), at("other")])]);
    expect(result.notes).toEqual([
      { paragraphOf: "s1", text: "Section Port was removed from section_01.md", path: FILE },
    ]);
  });

  it("nested level-1 run moves among the new parent's cited sentences", () => {
    const prev = page([sentence("o1", [at("a"), at("b")], ["x1"])]);
    const next = page([sentence("o2", [at("a"), at("b")], ["x7", "x8"])]);
    const cited: Record<string, Sentence> = {
      x7: sentence("x7", [at("a")]),
      x8: sentence("x8", [at("b")]),
    };
    const open: OpenRun[] = [
      { level: 0, sentenceId: "o1", restsOn: [at("a"), at("b")], parent: null },
      run("x1", [at("b")], "o1"),
    ];
    const result = carryOpenState(prev, next, open, (_summary, id) => cited[id]);
    expect(result.open).toEqual([
      { level: 0, sentenceId: "o2", restsOn: [at("a"), at("b")], parent: null },
      run("x8", [at("b")], "o2"),
    ]);
  });

  // The column writes only level 0 and 1 runs, resting on the sentence's own anchors.
  it("kept level-1 run notes a section dropped from its rests_on", () => {
    const prev = page([sentence("s0", [at("y")]), sentence("s1", [at("a"), at("b")])]);
    const next = page([sentence("s0", [at("y")]), sentence("s1", [at("a")])]);
    const result = carryOpenState(prev, next, [run("s1", [at("a"), at("b")])]);
    expect(result.open).toEqual([run("s1", [at("a")])]);
    expect(result.notes).toEqual([
      { paragraphOf: "s1", text: "Section b was removed from section_01.md", path: FILE },
    ]);
  });

  it("moved level-1 run notes a dropped section", () => {
    const prev = page([sentence("s1", [at("a"), at("b"), at("c")])]);
    const next = page([sentence("t1", [at("x")]), sentence("t2", [at("a"), at("b")])]);
    const result = carryOpenState(prev, next, [run("s1", [at("a"), at("b"), at("c")])]);
    expect(result.open).toEqual([run("t2", [at("a"), at("b")])]);
    expect(result.notes).toEqual([
      { paragraphOf: "t2", text: "Section c was removed from section_01.md", path: FILE },
    ]);
  });

  it("nested level-1 run notes a dropped section after its top-level sentence", () => {
    const prev = page([sentence("o1", [at("a"), at("b")], ["x1"])]);
    const next = page([sentence("o1", [at("a")], ["x1"])]);
    const cited: Record<string, Sentence> = { x1: sentence("x1", [at("a")]) };
    const open: OpenRun[] = [
      { level: 0, sentenceId: "o1", restsOn: [at("a"), at("b")], parent: null },
      run("x1", [at("a"), at("b")], "o1"),
    ];
    const result = carryOpenState(prev, next, open, (_summary, id) => cited[id]);
    expect(result.open).toEqual([
      { level: 0, sentenceId: "o1", restsOn: [at("a")], parent: null },
      run("x1", [at("a")], "o1"),
    ]);
    expect(result.notes).toEqual([
      { paragraphOf: "o1", text: "Section b was removed from section_01.md", path: FILE },
    ]);
  });

  it("a section the rebuild stopped citing but the file still has gets the rewritten note", () => {
    const prev = page([sentence("s0", [at("y")]), sentence("s1", [at("a"), at("b", "Bee")])]);
    const next = page([sentence("s0", [at("y")]), sentence("s1", [at("a")])]);
    const inFile = (anchor: SourceAnchorRef) => anchor.anchor === "b";
    const open = [run("s1", [at("a"), at("b", "Bee")])];
    const result = carryOpenState(prev, next, open, undefined, inFile);
    expect(result.open).toEqual([run("s1", [at("a")])]);
    expect(result.notes).toEqual([
      {
        paragraphOf: "s1",
        text: "This summary was rewritten and no longer draws on Bee",
        path: FILE,
      },
    ]);
  });

  it("a section gone from its file keeps the removed note", () => {
    const prev = page([sentence("s0", [at("y")]), sentence("s1", [at("a"), at("b", "Bee")])]);
    const next = page([sentence("s0", [at("y")]), sentence("s1", [at("a")])]);
    const open = [run("s1", [at("a"), at("b", "Bee")])];
    const result = carryOpenState(prev, next, open, undefined, () => false);
    expect(result.notes).toEqual([
      { paragraphOf: "s1", text: "Section Bee was removed from section_01.md", path: FILE },
    ]);
  });

  it("output follows the next page's reading order", () => {
    const prev = page([sentence("s1", [at("a")]), sentence("s2", [at("b")])]);
    const next = page([sentence("t1", [at("b")])], [sentence("t2", [at("a")])]);
    const result = carryOpenState(prev, next, [run("s1", [at("a")]), run("s2", [at("b")])]);
    expect(result.open.map((entry) => entry.sentenceId)).toEqual(["t1", "t2"]);
  });
});

describe("stored runs after a rebuild", () => {
  it("syntheticPrevious holds one sentence per stored run on its stored anchors", () => {
    const stored = [run("s1", [at("a", "Alpha")]), passage(at("a"), "s1")];
    const synthetic = syntheticPrevious(stored);
    expect(conceptPageSchema.safeParse(synthetic).success).toBe(true);
    const sentences = synthetic.blocks.flatMap((block) =>
      block.kind === "paragraph" ? block.sentences : [],
    );
    expect(sentences.map((entry) => [entry.id, entry.rests_on])).toEqual([
      ["s1", [{ path: FILE, anchor: "a" }]],
      [`${FILE}#a`, [{ path: FILE, anchor: "a" }]],
    ]);
  });

  it("carryStoredRuns leaves runs whose sentences are still on the page", () => {
    const current = page([sentence("s1", [at("a")])]);
    expect(carryStoredRuns(current, [run("s1", [at("a")])])).toBeNull();
    const moved = carryStoredRuns(current, [run("gone", [at("a")])]);
    expect(moved?.open).toEqual([run("s1", [at("a")])]);
  });
});
