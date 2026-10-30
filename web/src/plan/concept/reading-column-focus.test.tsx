import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import { cite, pageOf, S1, sent, src } from "./reading-column-fixtures";
import {
  active,
  blurOnRemoval,
  button,
  caps,
  installColumnTestHooks,
  mark,
  loadPage,
  mount,
  nested,
  pages,
  press,
  replace,
} from "./reading-column-harness";
import { openRunsAtom } from "./view-state";

installColumnTestHooks();

const PLANNING = [mark("section_01", "Planning", 1)];

/** Overview sentences a and b both cite x of section_01; x is opened to its passage under both. */
async function sharedCitation() {
  const overview = pageOf("overview/demo/product", [
    sent("a", "First overview.", [cite("section_01", "x")], PLANNING),
    sent("b", "Second overview.", [cite("section_01", "x")], PLANNING),
    sent("c", "Closing line."),
  ]);
  const detail = pageOf("demo/product/section_01", [
    sent("x", "Detail opens.", [src(S1, "intro")]),
  ]);
  pages["demo/product/section_01"] = detail;
  const view = mount("overview/demo/product", overview);
  loadPage(view.store, "demo/product/section_01", detail);
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

it("the arrows step over the caps of a run under its second citer, never onto a passage or plain sentence", async () => {
  const { first, second, passage } = await sharedCitation();
  // Second's start cap and x's start cap open the passage's chip line; x's end cap ends it.
  const [secondStart, xStart, xEnd, secondEnd] = caps(second);
  expect(passage.contains(secondStart!) && passage.contains(xStart!)).toBe(true);
  press("ArrowDown");
  await waitFor(() => expect(active()).toBe(xStart));
  press("ArrowDown");
  expect(active()).toBe(xEnd);
  press("ArrowDown");
  expect(active()).toBe(secondEnd);
  // "Closing line." opens nothing: the walk ends on the run's end cap.
  press("ArrowDown");
  expect(active()).toBe(secondEnd);
  press("ArrowUp");
  press("ArrowUp");
  press("ArrowUp");
  expect(active()).toBe(secondStart);
  expect(second.contains(active())).toBe(true);
  press("ArrowUp");
  expect(active()).toBe(caps(first).at(-1));
  expect(active().hasAttribute("data-passage")).toBe(false);
});

it("Shift+Escape from a nested cap folds every run and focuses the outermost opener", async () => {
  const { column } = nested();
  fireEvent.click(button(column, /^Overview opens/));
  fireEvent.click(await within(column).findByRole("button", { name: /^Detail opens/ }));
  await within(column).findByRole("group", { name: /^Detail of Detail/ });
  const inner = caps(column)[1]!;
  act(() => inner.focus());
  blurOnRemoval();
  expect(fireEvent.keyDown(inner, { key: "Escape", shiftKey: true })).toBe(false);
  expect(within(column).queryByRole("group")).toBeNull();
  await waitFor(() => expect(active()).toBe(button(column, /^Overview opens/)));
});

it("Escape on a passage under the second citer folds that citer's run only", async () => {
  const { first, second } = await sharedCitation();
  press("Escape");
  await waitFor(() => expect(second.querySelector("[data-passage]")).toBeNull());
  expect(first.querySelector("[data-passage]")).not.toBeNull();
  await waitFor(() => expect(active().textContent).toMatch(/^Detail opens/));
  expect(second.contains(active())).toBe(true);
});

async function siblingRuns() {
  const overview = pageOf("overview/demo/product", [
    sent("a", "First overview.", [cite("section_01", "x")], PLANNING),
    sent("b", "Second overview.", [cite("section_01", "x")], PLANNING),
    sent("c", "Closed sentence.", [src(S1, "intro")]),
  ]);
  const detail = pageOf("demo/product/section_01", [
    sent("x", "Detail opens.", [src(S1, "intro")]),
  ]);
  pages["demo/product/section_01"] = detail;
  const view = mount("overview/demo/product", overview);
  loadPage(view.store, "demo/product/section_01", detail);
  const { column } = view;
  fireEvent.click(button(column, /^First overview/));
  const first = await within(column).findByRole("group", { name: /^Detail of First/ });
  fireEvent.click(button(column, /^Second overview/));
  const second = await within(column).findByRole("group", { name: /^Detail of Second/ });
  const closed = button(column, /^Closed sentence/);
  closed.focus();
  await waitFor(() => expect(active()).toBe(closed));
  expect(view.store.get(openRunsAtom("overview/demo/product"))).toHaveLength(2);
  return { ...view, first, second, closed };
}

it("Escape outside any run folds the most recently opened run", async () => {
  const { column, first, second, store } = await siblingRuns();
  expect(press("Escape")).toBe(false);
  await waitFor(() => expect(column.contains(second)).toBe(false));
  expect(column.contains(first)).toBe(true);
  expect(store.get(openRunsAtom("overview/demo/product"))).toHaveLength(1);
  await waitFor(() => expect(active().textContent).toMatch(/^Second overview/));
});

it("Shift+Escape folds every open run", async () => {
  const { column, first, second, store } = await siblingRuns();
  expect(fireEvent.keyDown(active(), { key: "Escape", shiftKey: true })).toBe(false);
  await waitFor(() => {
    expect(column.contains(first)).toBe(false);
    expect(column.contains(second)).toBe(false);
  });
  expect(store.get(openRunsAtom("overview/demo/product"))).toEqual([]);
});

it("ArrowLeft outside any run folds nothing", async () => {
  const { column, first, second, closed, store } = await siblingRuns();
  expect(press("ArrowLeft")).toBe(true);
  expect(column.contains(first)).toBe(true);
  expect(column.contains(second)).toBe(true);
  expect(store.get(openRunsAtom("overview/demo/product"))).toHaveLength(2);
  expect(active()).toBe(closed);
});

it("a rebuild renaming the open sentence keeps focus on its cited sentence", async () => {
  const rests_on = [
    { path: S1, anchor: "intro" },
    { path: S1, anchor: "detail" },
  ];
  const cites = [cite("section_01", "x1"), cite("section_01", "x2")];
  const overview = (id: string) => ({ ...sent(id, "Overview opens.", cites, PLANNING), rests_on });
  const detail = pageOf("demo/product/section_01", [
    sent("x1", "First detail.", [src(S1, "intro")]),
    sent("x2", "Second detail.", [src(S1, "detail")]),
  ]);
  pages["demo/product/section_01"] = detail;
  const rest = sent("r", "Rest here.");
  const { column, store } = mount(
    "overview/demo/product",
    pageOf("overview/demo/product", [overview("o"), rest]),
  );
  loadPage(store, "demo/product/section_01", detail);
  fireEvent.click(button(column, /^Overview opens/));
  await within(column).findByRole("button", { name: /^First detail/ });
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^First detail/));
  // Same sources, new id: the first commit shows o2 closed, the carry reopens it.
  replace(store, "overview/demo/product", pageOf("overview/demo/product", [overview("o2"), rest]));
  await waitFor(() => expect(active().textContent).toMatch(/^First detail/));
  expect(active().closest('[role="group"]')).not.toBeNull();
});
