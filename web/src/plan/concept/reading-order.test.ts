import { describe, expect, it } from "vitest";
import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import { columnPage } from "./fragments";
import type { CitedLookup } from "./open-state";
import type { OpenRun } from "./types";
import { readingOrder } from "./reading-order";

const sentence = (id: string, citations: Sentence["citations"] = []): Sentence => ({
  id,
  text: id,
  marks: [],
  citations,
  rests_on: [],
  plans: [],
});
const page = (sentences: Sentence[]): ConceptPage => ({
  feature: "demo",
  key: "overview/demo/product",
  title: "Product",
  path: null,
  file_hash: null,
  colour: null,
  state: null,
  plan_state: null,
  last_pressure: null,
  key_chips: [],
  blocks: [
    { kind: "heading", level: 1, text: "Heading" },
    { kind: "paragraph", sentences },
  ],
  unlisted: [],
  implements: null,
  implemented_by: [],
  empty: null,
  building: null,
  estimate: null,
});
const run = (sentenceId: string, level: 0 | 1, parent: string | null = null): OpenRun => ({
  sentenceId,
  level,
  parent,
  restsOn: [],
});
const cite = (id: string): Sentence["citations"][number] => ({
  kind: "sentence",
  summary: "detail",
  id,
});

/** Every cited sentence resolves, as a loaded summary page would resolve it. */
const known: CitedLookup = (_summary, id) => sentence(id);

/** A sentence that rests on and cites the sections `anchors` of doc/a.md. */
const sourceSentence = (id: string, anchors: string[]): Sentence => ({
  ...sentence(
    id,
    anchors.map((anchor) => ({ kind: "source", path: "doc/a.md", anchor })),
  ),
  rests_on: anchors.map((anchor) => ({ path: "doc/a.md", anchor })),
});

describe("readingOrder", () => {
  it("lists a closed page in document order", () => {
    expect(readingOrder(page([sentence("a", [cite("x")]), sentence("b")]), [], {}, known)).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "b", level: 0, runOf: null },
    ]);
  });

  it("lists an open level-0 run in citation order", () => {
    expect(
      readingOrder(
        page([sentence("a", [cite("y"), cite("x")]), sentence("b")]),
        [run("a", 0)],
        {},
        known,
      ),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "y", level: 1, runOf: "a" },
      { kind: "sentence", id: "x", level: 1, runOf: "a" },
      { kind: "sentence", id: "b", level: 0, runOf: null },
    ]);
  });

  it("lists two passages within a nested level-1 run", () => {
    expect(
      readingOrder(
        page([sentence("a", [cite("x"), cite("y")])]),
        [run("a", 0), run("x", 1, "a")],
        { "a|x": ["doc/a.md#one", "doc/b.md#two"] },
        known,
      ),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "x", level: 1, runOf: "a" },
      { kind: "passage", id: "doc/a.md#one", level: 2, runOf: "x" },
      { kind: "passage", id: "doc/b.md#two", level: 2, runOf: "x" },
      { kind: "sentence", id: "y", level: 1, runOf: "a" },
    ]);
  });

  it("omits folded runs even if a nested run remains open", () => {
    expect(
      readingOrder(
        page([sentence("a", [cite("x")])]),
        [run("x", 1, "a")],
        { "a|x": ["doc/a.md#one"] },
        known,
      ),
    ).toEqual([{ kind: "sentence", id: "a", level: 0, runOf: null }]);
  });

  it("expands a sentence cited by two parents only under the parent it was opened from", () => {
    const order = readingOrder(
      page([sentence("a", [cite("x")]), sentence("b", [cite("x")])]),
      [run("a", 0), run("b", 0), run("x", 1, "a")],
      { "a|x": ["doc/a.md#one"] },
      known,
    );
    expect(order).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "x", level: 1, runOf: "a" },
      { kind: "passage", id: "doc/a.md#one", level: 2, runOf: "x" },
      { kind: "sentence", id: "b", level: 0, runOf: null },
      { kind: "sentence", id: "x", level: 1, runOf: "b" },
    ]);
  });

  it("does not take a top-level run for a cited sentence with the same id", () => {
    const order = readingOrder(
      page([sentence("x"), sentence("a", [cite("x")])]),
      [run("x", 1), run("a", 0)],
      { "|x": ["doc/a.md#one"] },
      known,
    );
    expect(order.filter((target) => target.kind === "passage")).toEqual([
      { kind: "passage", id: "doc/a.md#one", level: 1, runOf: "x" },
    ]);
  });

  it("opens a level-0 source citation directly to passages", () => {
    const source: Sentence["citations"][number] = {
      kind: "source",
      path: "doc/a.md",
      anchor: "one",
    };
    expect(
      readingOrder(
        page([sentence("a", [source])]),
        [run("a", 0)],
        { "|a": ["doc/a.md#one"] },
        known,
      ),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "passage", id: "doc/a.md#one", level: 1, runOf: "a" },
    ]);
  });

  it("lists a merged fragment as one sentence with the union of its passages", () => {
    const merged = columnPage(
      page([sourceSentence("a", ["one"]), sourceSentence("b", ["one", "two"])]),
    );
    expect(
      readingOrder(merged, [run("a", 0)], { "|a": ["doc/a.md#one", "doc/a.md#two"] }, known),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "passage", id: "doc/a.md#one", level: 1, runOf: "a" },
      { kind: "passage", id: "doc/a.md#two", level: 1, runOf: "a" },
    ]);
  });

  it("lists adjacent cited sentences that share a section as one nested entry", () => {
    const cited: Record<string, Sentence> = {
      x: sourceSentence("x", ["one"]),
      y: sourceSentence("y", ["one"]),
    };
    const lookup: CitedLookup = (_summary, id) => cited[id];
    expect(
      readingOrder(
        page([sentence("a", [cite("x"), cite("y")])]),
        [run("a", 0), run("x", 1, "a")],
        { "a|x": ["doc/a.md#one"] },
        lookup,
      ),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "x", level: 1, runOf: "a" },
      { kind: "passage", id: "doc/a.md#one", level: 2, runOf: "x" },
    ]);
  });
});
