import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import { cite, pageOf, S1, sent, src } from "./reading-column-fixtures";
import {
  active,
  blurOnRemoval,
  button,
  caps,
  installColumnTestHooks,
  loadPage,
  mark,
  mount,
  openable,
  pages,
  replace,
} from "./reading-column-harness";

installColumnTestHooks();

const OVERVIEW = "overview/demo/product";
const SUMMARY = "demo/product/section_01";

/**
 * "Zulu" leads the page; "o" cites m1 (intro) and m2 (intro and detail) of section_01, which
 * merge into one fragment led by m1 whose run rests on both sections.
 */
const overview = () =>
  pageOf(OVERVIEW, [
    openable("z", "Zulu opens."),
    sent(
      "o",
      "Overview opens.",
      [cite("section_01", "m1"), cite("section_01", "m2")],
      [mark("section_01", "Planning", 1)],
    ),
  ]);

it("a page replacement keeps an open fragment's run on its lead, so folding it focuses the fragment", async () => {
  const summary = pageOf(SUMMARY, [
    sent("m1", "Mike one opens.", [src(S1, "intro")]),
    sent("m2", "Mike two opens.", [src(S1, "intro"), src(S1, "detail")]),
  ]);
  pages[SUMMARY] = summary;
  const { column, store } = mount(OVERVIEW, overview());
  loadPage(store, SUMMARY, summary);
  fireEvent.click(button(column, /^Overview opens/));
  const outer = await within(column).findByRole("group", { name: /^Detail of/ });
  fireEvent.click(await within(outer).findByRole("button", { name: /^Mike one opens/ }));
  await within(outer).findByRole("group", { name: /^Detail of/ });

  // Any refetch hands the column a new page object, which runs the open-state carry.
  replace(store, OVERVIEW, overview());

  const inner = within(column)
    .getAllByRole("group", { name: /^Detail of/ })
    .find((run) => run !== outer && outer.contains(run))!;
  const cap = caps(inner).find((item) => item.getAttribute("data-side") === "end")!;
  act(() => cap.focus());
  blurOnRemoval();
  fireEvent.keyDown(cap, { key: "Escape" });

  await waitFor(() => {
    expect(within(outer).queryByRole("group", { name: /^Detail of/ })).toBeNull();
    expect(active().textContent).toMatch(/^Mike one opens/);
  });
});
