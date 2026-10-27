import { describe, expect, it } from "vitest";
import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
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
  key: "product",
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

describe("readingOrder", () => {
  it("lists a closed page in document order", () => {
    expect(readingOrder(page([sentence("a", [cite("x")]), sentence("b")]), [], {})).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "b", level: 0, runOf: null },
    ]);
  });

  it("lists an open level-0 run in citation order", () => {
    expect(
      readingOrder(page([sentence("a", [cite("y"), cite("x")]), sentence("b")]), [run("a", 0)], {}),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "sentence", id: "y", level: 1, runOf: "a" },
      { kind: "sentence", id: "x", level: 1, runOf: "a" },
      { kind: "sentence", id: "b", level: 0, runOf: null },
    ]);
  });

  it("lists two passages within a nested level-1 run", () => {
    expect(
      readingOrder(page([sentence("a", [cite("x"), cite("y")])]), [run("a", 0), run("x", 1, "a")], {
        x: ["doc/a.md#one", "doc/b.md#two"],
      }),
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
      readingOrder(page([sentence("a", [cite("x")])]), [run("x", 1, "a")], { x: ["doc/a.md#one"] }),
    ).toEqual([{ kind: "sentence", id: "a", level: 0, runOf: null }]);
  });

  it("opens a level-0 source citation directly to passages", () => {
    const source: Sentence["citations"][number] = {
      kind: "source",
      path: "doc/a.md",
      anchor: "one",
    };
    expect(
      readingOrder(page([sentence("a", [source])]), [run("a", 0)], { a: ["doc/a.md#one"] }),
    ).toEqual([
      { kind: "sentence", id: "a", level: 0, runOf: null },
      { kind: "passage", id: "doc/a.md#one", level: 1, runOf: "a" },
    ]);
  });
});
