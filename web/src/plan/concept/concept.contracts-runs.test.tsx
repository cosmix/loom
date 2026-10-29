import { expect, it } from "vitest";

import { sentence, summaryPage, type Anchor, type Page } from "./concept.contracts-kit";

type OpenRun = { level: 0 | 1 | 2; sentenceId: string; restsOn: Anchor[]; parent: string | null };
type FoldNote = { paragraphOf: string; text: string; path: string };
type OpenStateModule = {
  carryOpenState(prev: Page, next: Page, open: OpenRun[]): { open: OpenRun[]; notes: FoldNote[] };
};

const SECTION_01 = "doc/product/demo/section_01.md";
const SECTION_02 = "doc/product/demo/section_02.md";

async function loadOpenState(): Promise<OpenStateModule> {
  // A variable specifier keeps vite from resolving the module at collection, so a missing
  // open-state.ts fails inside the contracts that call it instead of hiding every other one.
  const specifier = "./open-state";
  return (await import(/* @vite-ignore */ specifier)) as OpenStateModule;
}

it("concept open run moves to the new sentence with the largest source overlap", async () => {
  const anchor = (name: string): Anchor => ({ path: SECTION_01, anchor: name });
  const [a, b, c, d] = [anchor("a"), anchor("b"), anchor("c"), anchor("d")];
  const prev = summaryPage("demo/product/section_01", [sentence("s1", "Old text.", [a, b, c])]);
  // The id-keyed and first-overlap rules both pick s1; the overlap rule picks s9.
  const next = summaryPage("demo/product/section_01", [
    sentence("s1", "Rewritten on a.", [a]),
    sentence("s9", "Rewritten on a, b, c and d.", [a, b, c, d]),
  ]);
  const open: OpenRun[] = [{ level: 1, sentenceId: "s1", restsOn: [a, b, c], parent: null }];
  const { carryOpenState } = await loadOpenState();
  const result = carryOpenState(prev, next, open);
  expect(result.open).toHaveLength(1);
  expect(result.open[0]).toMatchObject({ level: 1, sentenceId: "s9", parent: null });
  expect(result.open[0]!.restsOn.map((entry) => entry.anchor)).toEqual(["a", "b", "c", "d"]);
  expect(result.notes).toEqual([]);
});

it("concept open run folds with a removed section note when no sentence rests on it", async () => {
  const port: Anchor = { path: SECTION_02, anchor: "port", title: "Port" };
  const other: Anchor = { path: SECTION_02, anchor: "other" };
  const prev = summaryPage("demo/product/section_02", [
    sentence("s1", "Ports and more.", [{ path: SECTION_02, anchor: "port" }, other]),
  ]);
  const next = summaryPage("demo/product/section_02", [sentence("s1", "Only more.", [other])]);
  const open: OpenRun[] = [
    {
      level: 1,
      sentenceId: "s1",
      restsOn: [{ path: SECTION_02, anchor: "port" }, other],
      parent: null,
    },
    { level: 2, sentenceId: `${SECTION_02}#port`, restsOn: [port], parent: "s1" },
  ];
  const { carryOpenState } = await loadOpenState();
  const result = carryOpenState(prev, next, open);
  const onPort = result.open.filter(
    (run) => run.level === 2 && run.restsOn.some((entry) => entry.anchor === "port"),
  );
  expect(onPort).toEqual([]);
  expect(result.notes).toHaveLength(1);
  expect(result.notes[0]!.text).toContain("Port");
  expect(result.notes[0]!.text).toContain("section_02.md");
  expect(result.notes[0]!.path).toContain("section_02.md");
});
