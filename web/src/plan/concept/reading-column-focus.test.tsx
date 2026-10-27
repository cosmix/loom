import { fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import { cite, mark, pageOf, S1, sent, src } from "./reading-column-fixtures";
import {
  active,
  button,
  installColumnTestHooks,
  loadPage,
  mount,
  pages,
  press,
  replace,
} from "./reading-column-harness";

installColumnTestHooks();

const PLANNING = [mark("section_01", "Planning", 1)];

/** Overview sentences a and b both cite x of section_01; x is opened to its passage under both. */
async function sharedCitation() {
  const overview = pageOf("product", [
    sent("a", "First overview.", [cite("section_01", "x")], PLANNING),
    sent("b", "Second overview.", [cite("section_01", "x")], PLANNING),
    sent("c", "Closing line."),
  ]);
  const detail = pageOf("product/section_01", [sent("x", "Detail opens.", [src(S1, "intro")])]);
  pages["product/section_01"] = detail;
  const view = mount("product", overview);
  loadPage(view.store, "product/section_01", detail);
  const { column } = view;
  fireEvent.click(button(column, /^First overview/));
  const first = await within(column).findByRole("group", { name: /^Detail of First/ });
  fireEvent.click(await within(first).findByRole("button", { name: /^Detail opens/ }));
  fireEvent.click(button(column, /^Second overview/));
  const second = await within(column).findByRole("group", { name: /^Detail of Second/ });
  fireEvent.click(await within(second).findByRole("button", { name: /^Detail opens/ }));
  await waitFor(() => expect(first.querySelector("[data-passage]")).not.toBeNull());
  const passage = await waitFor(() => {
    const element = second.querySelector<HTMLElement>("[data-passage]");
    expect(element).not.toBeNull();
    return element!;
  });
  passage.focus();
  await waitFor(() => expect(active()).toBe(passage));
  return { ...view, first, second, passage };
}

it("the arrows walk the passages of a sentence opened under its second citer", async () => {
  const { second, passage } = await sharedCitation();
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toBe("Closing line."));
  press("ArrowUp");
  await waitFor(() => expect(active()).toBe(passage));
  press("ArrowUp");
  await waitFor(() => expect(active().getAttribute("aria-label")).toBe("Detail of Detail opens."));
  expect(second.contains(active())).toBe(true);
});

it("Escape on a passage under the second citer folds that citer's run only", async () => {
  const { first, second } = await sharedCitation();
  press("Escape");
  await waitFor(() => expect(second.querySelector("[data-passage]")).toBeNull());
  expect(first.querySelector("[data-passage]")).not.toBeNull();
  await waitFor(() => expect(active().textContent).toMatch(/^Detail opens/));
  expect(second.contains(active())).toBe(true);
});

it("a rebuild renaming the open sentence keeps focus on its cited sentence", async () => {
  const rests_on = [
    { path: S1, anchor: "intro" },
    { path: S1, anchor: "detail" },
  ];
  const cites = [cite("section_01", "x1"), cite("section_01", "x2")];
  const overview = (id: string) => ({ ...sent(id, "Overview opens.", cites, PLANNING), rests_on });
  const detail = pageOf("product/section_01", [
    sent("x1", "First detail.", [src(S1, "intro")]),
    sent("x2", "Second detail.", [src(S1, "detail")]),
  ]);
  pages["product/section_01"] = detail;
  const rest = sent("r", "Rest here.");
  const { column, store } = mount("product", pageOf("product", [overview("o"), rest]));
  loadPage(store, "product/section_01", detail);
  fireEvent.click(button(column, /^Overview opens/));
  await within(column).findByRole("button", { name: /^First detail/ });
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^First detail/));
  // Same sources, new id: the first commit shows o2 closed, the carry reopens it.
  replace(store, "product", pageOf("product", [overview("o2"), rest]));
  await waitFor(() => expect(active().textContent).toMatch(/^First detail/));
  expect(active().closest('[role="group"]')).not.toBeNull();
});
