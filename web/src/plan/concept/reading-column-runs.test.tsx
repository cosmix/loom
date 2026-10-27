import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { conceptPageAtom } from "./page-state";
import { sectionInShownDoc } from "./passage-doc";
import { docViewAtom, refreshDocView } from "./passages";
import { cite, mark, pageOf, rect, S1, S2, sent, src } from "./reading-column-fixtures";
import {
  active,
  button,
  docs,
  group,
  installColumnTestHooks,
  loadPage,
  mount,
  mountProduct,
  pages,
  press,
  replace,
  two,
} from "./reading-column-harness";

installColumnTestHooks();

it("plan glyph shows when the run starts at a paragraph's second sentence", async () => {
  const plans = [{ slug: "feat-x", title: "Feature X", state: "running" }];
  const marks = [mark("section_01", "Planning", 1)];
  const overview = pageOf("product", [
    sent("o", "Overview opens.", [cite("section_01", "x2")], marks),
  ]);
  // The server sets `plans` only on a paragraph's first sentence; x2 is the second.
  const x1 = { ...sent("x1", "First detail."), plans };
  const detail = pageOf("product/section_01", [x1, sent("x2", "Second.", [src(S1, "intro")])]);
  pages["product/section_01"] = detail;
  const { column, store } = mount("product", overview);
  loadPage(store, "product/section_01", detail);
  fireEvent.click(button(column, /^Overview opens/));
  const run = await group(column);
  expect(within(run).getByRole("button", { name: "Plans implementing this section" })).toBeTruthy();
});

it("d on an opened run opens that sentence's own first document", async () => {
  const { column, router } = mount(
    "product",
    pageOf("product", [
      sent("a", "First claim.", [src(S1, "intro")], [mark("section_01", "Planning", 1)]),
      sent("b", "Second claim.", [src(S2, "intro")], [mark("section_02", "Review", 2)]),
    ]),
  );
  fireEvent.click(button(column, /^Second claim/));
  await waitFor(() => expect(active().getAttribute("role")).toBe("group"));
  press("d");
  expect(router.state.location.pathname).toBe("/product/section_02");
});

it("a cited sentence gone from its loaded page links to that summary", async () => {
  const marks = [mark("section_01", "Planning", 1)];
  const overview = pageOf("product", [
    sent("o", "Overview opens.", [cite("section_01", "x")], marks),
  ]);
  // The member summary was rebuilt before the overview: sentence x is gone from its page.
  const rebuilt = pageOf("product/section_01", [sent("x9", "Rebuilt detail.")]);
  pages["product/section_01"] = rebuilt;
  const { column, store } = mount("product", overview);
  loadPage(store, "product/section_01", rebuilt);
  fireEvent.click(button(column, /^Overview opens/));
  const run = await group(column);
  expect(within(run).queryByText("Loading")).toBeNull();
  expect(within(run).getByText(/summary has been rebuilt since this overview/)).toBeTruthy();
  const link = within(run).getByRole("link", { name: /Read the Planning summary/ });
  expect(link.getAttribute("href")).toBe("/product/section_01");
});

it("Space opens a sentence button and is left alone on a plain sentence", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("b", "Plain one."), sent("a", "Spaced one.", [src(S2, "intro")])]),
  );
  column.focus();
  await waitFor(() => expect(active().textContent).toBe("Plain one."));
  expect(fireEvent.keyDown(active(), { key: " " })).toBe(true);
  press("j");
  await waitFor(() => expect(active().textContent).toMatch(/^Spaced one/));
  expect(fireEvent.keyDown(active(), { key: " " })).toBe(false);
  expect(await group(column)).toBeTruthy();
});

it("a on a passage whose file cannot be read attaches nothing", async () => {
  const onAttach = vi.fn();
  const gone = [src("doc/product/gone.md", "intro")];
  const { column } = mount("product", pageOf("product", [sent("a", "Gone file.", gone)]), onAttach);
  fireEvent.click(button(column, /^Gone file/));
  await within(column).findByText("This file could not be loaded.");
  press("ArrowDown");
  await waitFor(() => expect(active().hasAttribute("data-passage")).toBe(true));
  press("a");
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  expect(onAttach).not.toHaveBeenCalled();
});

it("a dropped section still in its file is noted as no longer drawn on", async () => {
  const { column, store } = mount("product", pageOf("product", [sent("a", "Both parts.", two)]));
  fireEvent.click(button(column, /^Both parts/));
  await within(column).findByText("Beta text");
  replace(store, "product", pageOf("product", [sent("a", "Intro only.", [src(S2, "intro")])]));
  expect(within(column).getByRole("note").textContent).toMatch(
    /^This summary was rewritten and no longer draws on detail/,
  );
});

it("a rebuild reopening a run above the focused sentence keeps it in place", async () => {
  let scrolled = 0;
  vi.spyOn(window, "scrollBy").mockImplementation((_x?: unknown, y?: unknown) => {
    scrolled += Number(y ?? 0);
  });
  // The focused sentence "b" sits 200px lower while a run is open above it.
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const opened = document.querySelector('[role="group"]') !== null;
    const layout = this.getAttribute("data-sentence-id") === "b" ? (opened ? 300 : 100) : 0;
    return rect(layout - scrolled);
  });
  const above = (id: string) => sent(id, "Above it.", [src(S2, "intro")]);
  const { column, store } = mount("product", pageOf("product", [above("a"), sent("b", "Here.")]));
  fireEvent.click(button(column, /^Above it/));
  await group(column);
  within(column).getByText("Here.").focus();
  await waitFor(() => expect(active().textContent).toBe("Here."));
  const top = () => within(column).getByText("Here.").getBoundingClientRect().top;
  expect(top()).toBe(300);
  // The rebuild renames the open sentence: the first commit closes its run, the carry reopens it.
  replace(store, "product", pageOf("product", [above("a2"), sent("b", "Here.")]));
  expect(within(column).getByRole("group")).toBeTruthy();
  expect(top()).toBe(300);
});

it("a dropped section of a file whose refetch failed is noted as removed", async () => {
  const { column, store } = mountProduct([
    sent("a", "Two files.", [src(S2, "intro"), src(S1, "intro")]),
  ]);
  fireEvent.click(button(column, /^Two files/));
  expect(await within(column).findAllByText("Alpha text")).toHaveLength(2);
  // The file is deleted: its refetch answers 404 and the kept document no longer counts.
  delete docs["product/section_02.md"];
  act(() => refreshDocView(store, S2));
  await act(() => store.get(docViewAtom(S2)).catch(() => undefined));
  const intro = { path: S2, anchor: "intro" };
  await waitFor(() => expect(sectionInShownDoc(store, intro)).toBeUndefined());
  replace(store, "product", pageOf("product", [sent("a2", "One file.", [src(S1, "intro")])]));
  expect(within(column).getByRole("note").textContent).toMatch(
    /^Section intro was removed from section_02\.md/,
  );
});

it("an open passage whose refetch failed says the file could not be loaded", async () => {
  const { column, store } = mountProduct([sent("a", "One file.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^One file/));
  await within(column).findByText("Alpha text");
  // The file is deleted: its refetch answers 404, so the kept section must not stay on screen.
  delete docs["product/section_02.md"];
  act(() => refreshDocView(store, S2));
  await act(() => store.get(docViewAtom(S2)).catch(() => undefined));
  expect(await within(column).findByText("This file could not be loaded.")).toBeTruthy();
  expect(within(column).queryByText("Alpha text")).toBeNull();
});

it("a file's shown document goes with its last passage", async () => {
  const { column, store } = mountProduct([sent("a", "Folded file.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Folded file/));
  await within(column).findByText("Alpha text");
  const intro = { path: S2, anchor: "intro" };
  await waitFor(() => expect(sectionInShownDoc(store, intro)).toBe(true));
  fireEvent.click(button(column, "Fold"));
  await waitFor(() => expect(sectionInShownDoc(store, intro)).toBeUndefined());
});

it("a cited summary page that no longer exists says so and links to it", async () => {
  const marks = [mark("section_01", "Planning", 1)];
  // No page is served for section_01: the column's prefetch of it answers 404.
  const { column } = mountProduct([sent("o", "Overview opens.", [cite("section_01", "x")], marks)]);
  fireEvent.click(button(column, /^Overview opens/));
  const run = await group(column);
  expect(await within(run).findByText(/^The Planning summary no longer exists/)).toBeTruthy();
  expect(within(run).queryByText("Loading")).toBeNull();
  const link = within(run).getByRole("link", { name: /Read the Planning summary/ });
  expect(link.getAttribute("href")).toBe("/product/section_01");
});

it("a cited summary page that failed to load says it could not be loaded", async () => {
  const marks = [mark("section_01", "Planning", 1)];
  const { column, store } = mountProduct([
    sent("o", "Overview opens.", [cite("section_01", "x")], marks),
  ]);
  const cited = conceptPageAtom("product/section_01");
  await waitFor(() => expect(store.get(cited).error).not.toBeNull());
  act(() => store.set(cited, { page: null, previous: null, error: "Server error", status: 500 }));
  fireEvent.click(button(column, /^Overview opens/));
  const run = await group(column);
  expect(within(run).getByText(/^The Planning summary could not be loaded/)).toBeTruthy();
  expect(within(run).getByRole("link", { name: /Read the Planning summary/ })).toBeTruthy();
});
