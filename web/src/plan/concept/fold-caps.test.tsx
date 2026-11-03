import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import runsSheet from "@/plan/concept/concept-runs.css?raw";

import {
  blurOnRemoval,
  caps as foldCaps,
  installColumnTestHooks,
  mount,
  pages,
} from "./reading-column-harness";

installColumnTestHooks();

const pageKey = "demo/product/section_01";
const firstSentence = "A plan starts from a short statement of intent.";
const secondSentence = "Each stage names the files it may change.";

it("both caps of each opened run are named Fold and tabbable in reading order", () => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  fireEvent.click(within(column).getByRole("button", { name: secondSentence }));

  const runs = within(column).getAllByRole("group", { name: /^Detail of/ });
  const caps = within(column).getAllByRole("button", { name: "Fold" });
  expect(caps.map((cap) => [cap.tabIndex, cap.getAttribute("data-side")])).toEqual([
    [0, "start"],
    [0, "end"],
    [0, "start"],
    [0, "end"],
  ]);
  expect(caps.map((cap) => cap.getAttribute("aria-describedby"))).toEqual([
    runs[0]!.id,
    runs[0]!.id,
    runs[1]!.id,
    runs[1]!.id,
  ]);
});

it.each(["start", "end"])("the %s cap shows Fold · Esc on focus", async (side) => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  const cap = within(column)
    .getAllByRole("button", { name: "Fold" })
    .find((item) => item.getAttribute("data-side") === side)!;

  act(() => cap.focus());

  expect(await screen.findByRole("tooltip", { name: "Fold · Esc" })).toBeTruthy();
});

it.each([
  ["start", "Enter"],
  ["end", "Enter"],
  ["start", " "],
  ["end", " "],
  ["start", "Escape"],
  ["end", "Escape"],
])("the %s cap folds with %s and restores sentence focus", async (side, key) => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  const cap = within(column)
    .getAllByRole("button", { name: "Fold" })
    .find((item) => item.getAttribute("data-side") === side)!;
  act(() => cap.focus());

  fireEvent.keyDown(cap, { key });

  expect(within(column).queryByRole("group", { name: /^Detail of/ })).toBeNull();
  await waitFor(() => {
    expect(document.activeElement).toBe(
      within(column).getByRole("button", { name: firstSentence }),
    );
  });
});

it("Shift+Escape from a cap folds all runs", () => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  fireEvent.click(within(column).getByRole("button", { name: secondSentence }));
  const cap = within(column).getAllByRole("button", { name: "Fold" })[0]!;

  fireEvent.keyDown(cap, { key: "Escape", shiftKey: true });

  expect(within(column).queryByRole("group", { name: /^Detail of/ })).toBeNull();
});

it("Shift+Escape from the first run's cap returns focus to the sentence that opened it", async () => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  fireEvent.click(within(column).getByRole("button", { name: secondSentence }));
  const cap = foldCaps(column).find((item) => item.getAttribute("data-side") === "end")!;
  act(() => cap.focus());
  blurOnRemoval();

  expect(fireEvent.keyDown(cap, { key: "Escape", shiftKey: true })).toBe(false);

  expect(within(column).queryByRole("group", { name: /^Detail of/ })).toBeNull();
  await waitFor(() => {
    expect(document.activeElement).toBe(
      within(column).getByRole("button", { name: firstSentence }),
    );
  });
});

it("an opened passage carries its caps inline: start before the chip, end after the body", () => {
  const { column } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));

  const run = within(column).getByRole("group", { name: /^Detail of/ });
  const passages = run.querySelectorAll<HTMLElement>(".concept-passage");
  const [start, end] = foldCaps(run);
  const chip = passages[0]!.querySelector(".concept-passage-chip")!;
  expect(run.querySelector(".concept-run-edge")).toBeNull();
  expect(start!.parentElement).toBe(passages[0]);
  expect(start!.nextElementSibling).toBe(chip);
  expect(passages[passages.length - 1]!.lastElementChild).toBe(end);
});

it("the enclosing start cap shares the first passage's chip line when its first citation opens", async () => {
  const page = conceptPageSchema.parse(summaryFixture);
  const cited = conceptPageSchema.parse({ ...summaryFixture, key: "demo/product/section_02" });
  const paragraph = page.blocks.find((block) => block.kind === "paragraph")!;
  paragraph.sentences[0]!.citations = [{ kind: "sentence", summary: cited.key, id: "s1" }];
  // The overview names the cited document, as the server's page does for every summary it can
  // reach: the column prefetches only the pages the overview names.
  const doc = {
    id: cited.key,
    title: "Planning",
    path: "doc/product/demo/section_02.md",
    page: cited.key,
  };
  paragraph.sentences[0]!.marks = [{ doc, colour: 1, out_of_date: null }];
  pages[cited.key] = cited;
  const { column } = mount(pageKey, page);
  fireEvent.click(within(column).getByRole("button", { name: firstSentence }));
  const outer = within(column).getByRole("group", { name: /^Detail of/ });
  fireEvent.click(await within(outer).findByRole("button", { name: firstSentence }));

  const inner = within(outer).getByRole("group", { name: /^Detail of/ });
  const caps = within(outer).getAllByRole("button", { name: "Fold" });
  expect(caps.map((cap) => cap.getAttribute("data-side"))).toEqual([
    "start",
    "start",
    "end",
    "end",
  ]);
  const firstPassage = inner.querySelector(".concept-passage")!;
  expect(caps[0]!.closest(".concept-run-leading")?.parentElement).toBe(firstPassage);
  expect(caps[1]!.parentElement).toBe(firstPassage);
  expect(caps[1]!.nextElementSibling).toBe(firstPassage.querySelector(".concept-passage-chip"));
  expect(caps[0]!.getAttribute("aria-describedby")).toBe(outer.id);
  expect(caps[3]!.parentElement).toBe(outer);
});

/** Every declaration of every rule whose selector list names `selector` on its own. */
function declarationsOf(sheet: string, selector: string): string[] {
  const css = sheet.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: string[] = [];
  for (const [, selectors = "", body = ""] of css.matchAll(/([^{};]+)\{([^{}]*)\}/g)) {
    if (!selectors.split(",").some((part) => part.trim() === selector)) continue;
    for (const decl of body.split(";")) {
      const normal = decl.replace(/\s+/g, " ").trim().toLowerCase();
      if (normal) found.push(normal.replace(/\s*:\s*/, ": "));
    }
  }
  return found;
}

it("each cited sentence keeps its level 1 hairline after an earlier member opens to passages", () => {
  const css = runsSheet.replace(/\/\*[\s\S]*?\*\//g, "");
  // The run-wide rule that dropped the hairline once the first member opened is gone...
  expect(css).not.toMatch(/\.concept-run:has\(\s*>\s*\.concept-passages:first-child\s*\)/);
  // ...and the hairline is drawn under each sentence the run holds, not under the run's span.
  expect(declarationsOf(runsSheet, ".concept-run > .concept-sentence-wrap")).toContain(
    "border-bottom: 1px solid var(--run-edge)",
  );
  expect(declarationsOf(runsSheet, ".concept-run").join(";")).not.toMatch(/border-bottom/);
});
