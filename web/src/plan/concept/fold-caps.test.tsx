import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";

import { installColumnTestHooks, mount, pages } from "./reading-column-harness";

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

it("the enclosing start cap shares the first passage edge when its first citation opens", async () => {
  const page = conceptPageSchema.parse(summaryFixture);
  const cited = conceptPageSchema.parse({ ...summaryFixture, key: "demo/product/section_02" });
  const paragraph = page.blocks.find((block) => block.kind === "paragraph")!;
  paragraph.sentences[0]!.citations = [{ kind: "sentence", summary: cited.key, id: "s1" }];
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
  expect(caps[0]!.closest('[data-side="start"].concept-run-edge')?.parentElement).toBe(inner);
  expect(caps[0]!.getAttribute("aria-describedby")).toBe(outer.id);
  expect(caps[3]!.parentElement).toBe(outer);
});
