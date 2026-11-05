import { describe, expect, it } from "vitest";

import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";

import { sentenceKey } from "./column-context";
import { findSite, movedRun, passagesRecord, survivorKey } from "./column-model";
import { columnPage } from "./fragments";
import type { CitedLookup } from "./open-state";
import type { ColumnTarget, OpenRun } from "./types";

const INTRO = { path: "doc/a.md", anchor: "intro" };
const run = (sentenceId: string, restsOn = [INTRO]): OpenRun => ({
  level: 0,
  sentenceId,
  restsOn,
  parent: null,
});
const top = (id: string): ColumnTarget => ({ kind: "sentence", id, level: 0, runOf: null });
const cited = (id: string, runOf: string): ColumnTarget => ({
  kind: "sentence",
  id,
  level: 1,
  runOf,
});

describe("movedRun", () => {
  const keys = new Set([sentenceKey(null, "o2"), sentenceKey(null, "r")]);

  it("finds the run on the next page that rests on the lost run's sections", () => {
    expect(movedRun([run("o")], [run("o2")], "o", keys)).toEqual(run("o2"));
  });

  it("skips a run whose sentence left the page, as the runs before a carry are", () => {
    expect(movedRun([run("o")], [run("o")], "o", keys)).toBeUndefined();
  });

  it("skips a run that was already open at the top level before the rebuild", () => {
    // p rests on B and C, o on A and B; a rebuild renames o to o2 and p stays where it was.
    const [a, b, c] = ["a", "b", "c"].map((anchor) => ({ path: "doc/a.md", anchor }));
    const [p, o] = [run("p", [b!, c!]), run("o", [a!, b!])];
    const o2 = run("o2", [a!, b!]);
    const next = new Set([sentenceKey(null, "p"), sentenceKey(null, "o2")]);
    expect(movedRun([p, o], [p, o2], "o", next)).toEqual(o2);
  });

  it("prefers the run resting on the most of the lost run's sections", () => {
    const [a, b, c] = ["a", "b", "c"].map((anchor) => ({ path: "doc/a.md", anchor }));
    const lost = run("o", [a!, b!, c!]);
    const [one, two] = [run("n1", [a!]), run("n2", [b!, c!])];
    const next = new Set([sentenceKey(null, "n1"), sentenceKey(null, "n2")]);
    expect(movedRun([lost], [one, two], "o", next)).toEqual(two);
  });

  it("skips a run resting on other sections", () => {
    const other = [{ path: "doc/a.md", anchor: "other" }];
    expect(movedRun([run("o")], [run("o2", other)], "o", keys)).toBeUndefined();
  });
});

describe("survivorKey", () => {
  const order = [top("o"), cited("x1", "o"), cited("x2", "o"), top("r")];
  const prev = { order, open: [run("o")] };

  it("keeps the focus on the same cited sentence inside the run it moved to", () => {
    const next = {
      order: [top("o2"), cited("x1", "o2"), cited("x2", "o2"), top("r")],
      open: [run("o2")],
    };
    expect(survivorKey(prev, next, sentenceKey("o", "x1"))).toBe(sentenceKey("o2", "x1"));
  });

  it("takes the moved run's sentence when the cited sentence is gone from it", () => {
    const next = { order: [top("o2"), cited("y", "o2"), top("r")], open: [run("o2")] };
    expect(survivorKey(prev, next, sentenceKey("o", "x1"))).toBe(sentenceKey(null, "o2"));
  });

  it("falls back to the target nearest the old position when nothing moved", () => {
    const next = { order: [top("o2"), top("r")], open: [] };
    expect(survivorKey(prev, next, sentenceKey("o", "x1"))).toBe(sentenceKey(null, "r"));
  });
});

/** A sentence that cites and rests on the sections `anchors` of doc/a.md. */
const resting = (id: string, anchors: string[]): Sentence => ({
  id,
  text: id,
  marks: [],
  citations: anchors.map((anchor) => ({ kind: "source" as const, path: "doc/a.md", anchor })),
  rests_on: anchors.map((anchor) => ({ path: "doc/a.md", anchor })),
  plans: [],
});

const citing = (id: string, cited: string[]): Sentence => ({
  ...resting(id, []),
  citations: cited.map((target) => ({ kind: "sentence" as const, summary: "s", id: target })),
});

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
    blocks: [{ kind: "paragraph", sentences }],
    unlisted: [],
    implements: null,
    implemented_by: [],
    empty: null,
    building: null,
    estimate: null,
  };
}

describe("merged fragments", () => {
  const view = columnPage(pageOf([resting("a", ["one"]), resting("b", ["one", "two"])]));
  const none: CitedLookup = () => undefined;

  it("gives an open fragment the union of its members' passages, each once", () => {
    expect(passagesRecord(view, [run("a")], none)).toEqual({ a: ["doc/a.md#one", "doc/a.md#two"] });
  });

  it("opens a fragment when the run was stored under one of its members", () => {
    expect(passagesRecord(view, [run("b")], none)).toEqual({ a: ["doc/a.md#one", "doc/a.md#two"] });
  });

  it("finds a cited fragment's site as the merged sentence", () => {
    const cited: Record<string, Sentence> = { x: resting("x", ["one"]), y: resting("y", ["one"]) };
    const lookup: CitedLookup = (_summary, id) => cited[id];
    const merged = columnPage(pageOf([citing("o", ["x", "y"])]));
    const site = findSite(merged, lookup, "demo/product/section_01", "x", "o");
    expect(site?.sentence.text).toBe("x y");
    expect(findSite(merged, lookup, "demo/product/section_01", "y", "o")).toBeNull();
  });
});
