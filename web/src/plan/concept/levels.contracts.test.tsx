import { cleanup, fireEvent, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDoc from "@/plan/api/fixtures/files-doc.json";

import { clone, makeStubFetch, planPage, readingColumn, renderAt } from "./concept-route-support";

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
const fetched: string[] = [];

beforeEach(() => {
  pages = {};
  docs = {};
  fetched.length = 0;
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing: false }), fetched)));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

type Section = [title: string, anchor: string, start: number, end: number];

/** A document view of `path` holding `text`, with level-2 `sections`. */
function docView(path: string, title: string, text: string, sections: Section[]) {
  const rows = sections.map(([name, anchor, start_line, end_line]) => ({
    level: 2,
    title: name,
    anchor,
    start_line,
    end_line,
  }));
  return { ...clone(filesDoc), path, title, text, sections: rows, size: text.length };
}

/** A sentence citing `anchors` of the source document `path`. */
function sourceSentence(id: string, text: string, path: string, anchors: string[]) {
  return {
    id,
    text,
    marks: [] as unknown[],
    citations: anchors.map((anchor) => ({ kind: "source", path, anchor })),
    rests_on: anchors.map((anchor) => ({ path, anchor })),
    plans: [],
  };
}

const PX = "doc/plans/demo/PLAN-x.md";
const PLAN_TEXT =
  "# Plan: X\n\n## Stages\n\nThe parser runs first.\n\n## Risks\n\nThe schema may drift.\n";
const MARK = {
  doc: { id: "plans/x", title: "Plan x", path: PX, page: "demo/engineering/x" },
  colour: 1,
  out_of_date: null,
};

function servePlan(): void {
  const opening = {
    id: "o1",
    text: "Plan x builds the review flow in two stages.",
    marks: [MARK],
    citations: [
      { kind: "sentence", summary: "plans/x", id: "d1" },
      { kind: "sentence", summary: "plans/x", id: "d2" },
    ],
    rests_on: [
      { path: PX, anchor: "stages" },
      { path: PX, anchor: "risks" },
    ],
    plans: [],
  };
  const d1 = {
    ...sourceSentence("d1", "Stages run the parser first.", PX, ["stages"]),
    marks: [MARK],
  };
  const d2 = {
    ...sourceSentence("d2", "Risks stay with the schema.", PX, ["risks"]),
    marks: [MARK],
  };
  pages["demo/engineering/x"] = {
    ...planPage({}),
    key: "demo/engineering/x",
    title: "Plan x",
    path: PX,
    blocks: [
      { kind: "heading", level: 1, text: "Plan x" },
      { kind: "paragraph", sentences: [opening] },
    ],
    detail: [d1, d2],
  };
  docs["plans/demo/PLAN-x.md"] = docView(PX, "Plan: X", PLAN_TEXT, [
    ["Stages", "stages", 3, 6],
    ["Risks", "risks", 7, 9],
  ]);
}

it("a two level plan summary opens to its detail sentences and then to the source", async () => {
  servePlan();
  renderAt("/f/demo/engineering/x");
  const column = await readingColumn();
  const opening = await within(column).findByRole("button", {
    name: /^Plan x builds the review flow/,
  });
  expect(column.textContent).not.toContain("Stages run the parser first.");
  fireEvent.click(opening);
  const run = await within(column).findByRole("group", {
    name: "Detail of Plan x builds the review",
  });
  const first = await within(run).findByRole("button", { name: /^Stages run the parser first/ });
  within(run).getByRole("button", { name: /^Risks stay with the schema/ });
  fireEvent.click(first);
  const header = await within(column).findByRole("link", { name: /PLAN-x\.md.*Stages/ });
  const href = header.getAttribute("href") ?? "";
  expect(href.startsWith("/f/demo/source/plans/demo/PLAN-x.md"), href).toBe(true);
  expect(href).toContain("stages");
  await waitFor(() => expect(column.textContent).toContain("The parser runs first."));
  expect(column.textContent).not.toContain("The schema may drift.");
});

const S = "doc/product/demo/section_01.md";
const SECTION_TEXT =
  "# Planning\n\n## Intent\n\nA plan begins with intent.\n\n" +
  "## Stages\n\nStages list their files.\n\n## Risks\n\nRisks are named.\n";

/** Serves section 01 and its summary page holding one paragraph of `sentences`. */
function serveSection(sentences: unknown[]): void {
  docs["product/demo/section_01.md"] = docView(S, "Planning", SECTION_TEXT, [
    ["Intent", "intent", 3, 6],
    ["Stages", "stages", 7, 10],
    ["Risks", "risks", 11, 13],
  ]);
  pages["demo/product/section_01"] = {
    ...clone(summaryFixture),
    key: "demo/product/section_01",
    blocks: [
      { kind: "heading", level: 1, text: "Planning" },
      { kind: "paragraph", sentences },
    ],
  };
}

it("adjacent sentences that share a passage open as one fragment", async () => {
  serveSection([
    sourceSentence("a", "Intent comes first in every plan.", S, ["intent"]),
    sourceSentence("b", "Intent shapes the stages.", S, ["intent", "stages"]),
    sourceSentence("c", "Stages carry the file lists.", S, ["stages"]),
    sourceSentence("d", "Risks are listed last.", S, ["risks"]),
  ]);
  renderAt("/f/demo/product/section_01");
  const column = await readingColumn();
  const fragment = await within(column).findByRole("button", {
    name: /^Intent comes first in every plan/,
  });
  expect(column.querySelectorAll("[aria-expanded]")).toHaveLength(2);
  fireEvent.click(fragment);
  await within(column).findByRole("group", { name: "Detail of Intent comes first in every" });
  await within(column).findAllByText("Stages list their files.");
  await waitFor(() => {
    expect(within(column).getAllByText("A plan begins with intent.")).toHaveLength(1);
    expect(within(column).getAllByText("Stages list their files.")).toHaveLength(1);
  });
  expect(within(column).queryAllByText("Risks are named.")).toHaveLength(0);
});

it("a passage shown twice in one view shows a reference the second time", async () => {
  serveSection([
    sourceSentence("a2", "Stages carry the file lists.", S, ["stages"]),
    sourceSentence("b2", "Intent comes first in every plan.", S, ["intent"]),
    sourceSentence("c2", "Every stage names its files.", S, ["stages"]),
  ]);
  renderAt("/f/demo/product/section_01");
  const column = await readingColumn();
  const first = await within(column).findByRole("button", { name: /^Stages carry the file lists/ });
  fireEvent.click(first);
  await within(column).findByRole("group", { name: "Detail of Stages carry the file lists." });
  fireEvent.click(within(column).getByRole("button", { name: /^Every stage names its files/ }));
  await within(column).findByRole("group", { name: "Detail of Every stage names its files." });
  await within(column).findByRole("button", { name: /^Same passage as above/ });
  await waitFor(() =>
    expect(within(column).getAllByText("Stages list their files.")).toHaveLength(1),
  );
});
