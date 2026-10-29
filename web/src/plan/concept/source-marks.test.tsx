import { act, fireEvent, within } from "@testing-library/react";
import { expect, it } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import conceptCss from "@/plan/concept/concept.css?raw";
import { sourceMarksAtom } from "@/plan/settings/preferences";

import { installColumnTestHooks, mount } from "./reading-column-harness";

installColumnTestHooks();

const pageKey = "demo/product/section_01";
const hidingSelector =
  '.concept-column[data-source-marks="off"] .concept-sentence-wrap .concept-marks';

it("shows source marks by default", () => {
  const { column, store } = mount(pageKey, conceptPageSchema.parse(summaryFixture));

  expect(store.get(sourceMarksAtom)).toBe(true);
  expect(column.hasAttribute("data-source-marks")).toBe(false);
  expect(within(column).getAllByRole("button", { name: /^Planning: doc\// })).toHaveLength(3);
});

it("turning source marks off selects the CSS rule that hides sentence glyphs", () => {
  const { column, store } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  const mark = within(column).getAllByRole("button", { name: /^Planning: doc\// })[0]!;
  const marks = mark.closest(".concept-marks")!;

  act(() => store.set(sourceMarksAtom, false));

  expect(column.getAttribute("data-source-marks")).toBe("off");
  expect(marks.matches(hidingSelector)).toBe(true);
  expect(conceptCss).toContain(`${hidingSelector} {\n  display: none;\n}`);
});

it("turning source marks back on removes the hiding attribute", () => {
  const { column, store } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  act(() => store.set(sourceMarksAtom, false));

  act(() => store.set(sourceMarksAtom, true));

  expect(column.hasAttribute("data-source-marks")).toBe(false);
  expect(column.querySelector(hidingSelector)).toBeNull();
});

it.each(["click", "Enter"])("sentences still open by %s with source marks off", (method) => {
  const { column, store } = mount(pageKey, conceptPageSchema.parse(summaryFixture));
  act(() => store.set(sourceMarksAtom, false));
  const sentence = within(column).getByRole("button", {
    name: "A plan starts from a short statement of intent.",
  });

  if (method === "click") fireEvent.click(sentence);
  else fireEvent.keyDown(sentence, { key: "Enter" });

  expect(within(column).getByRole("group", { name: /^Detail of/ })).toBeTruthy();
  expect(conceptCss).toMatch(/text-decoration: underline 2px;/);
});
