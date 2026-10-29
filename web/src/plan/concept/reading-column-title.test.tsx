import { render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, it } from "vitest";

import { initClient } from "@/plan/api/client";
import type { ConceptPage } from "@/plan/api/schema-concept";

import { conceptPageAtom } from "./page-state";
import { ReadingColumn } from "./reading-column";
import { pageOf, sent } from "./reading-column-fixtures";
import { installColumnTestHooks } from "./reading-column-harness";

installColumnTestHooks();

const body = { kind: "paragraph", sentences: [sent("a", "Plans are reviewed.")] };

/** Mounts the column the way the route does: `dropTitle` is set only where a page heading shows the title. */
function mountColumn(pageKey: string, page: ConceptPage, dropTitle: boolean) {
  const store = createStore();
  initClient(store);
  store.set(conceptPageAtom(pageKey), { page, previous: null, error: null });
  const router = createMemoryRouter([
    {
      path: "*",
      element: <ReadingColumn pageKey={pageKey} page={page} dropTitle={dropTitle} />,
    },
  ]);
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { column: screen.getByRole("document", { name: "Reading column" }) };
}

function mountSummary(title: string, blocks: unknown[]) {
  const key = "demo/product/section_01";
  return mountColumn(key, pageOf(key, [], { title, blocks }), true);
}

it("drops a leading level-1 block that repeats the page title", () => {
  const { column } = mountSummary("Planning", [
    { kind: "heading", level: 1, text: "  Planning " },
    body,
  ]);

  expect(within(column).queryAllByRole("heading")).toHaveLength(0);
  expect(within(column).getByText("Plans are reviewed.")).toBeTruthy();
});

it("compares the title with collapsed whitespace", () => {
  const { column } = mountSummary("Plan   ning", [
    { kind: "heading", level: 1, text: "Plan ning" },
    body,
  ]);

  expect(within(column).queryAllByRole("heading")).toHaveLength(0);
});

it("keeps a leading level-1 heading with different text and later headings", () => {
  const { column } = mountSummary("Planning", [
    { kind: "heading", level: 1, text: "Other" },
    { kind: "heading", level: 1, text: "Planning" },
    body,
  ]);

  expect(
    within(column)
      .getAllByRole("heading")
      .map((heading) => heading.textContent),
  ).toEqual(["Other", "Planning"]);
});

it("keeps a matching heading that is not level 1 or not first", () => {
  const { column } = mountSummary("Planning", [
    { kind: "heading", level: 2, text: "Planning" },
    body,
    { kind: "heading", level: 1, text: "Planning" },
  ]);

  expect(within(column).getAllByRole("heading")).toHaveLength(2);
});

it("keeps the leading level-1 title heading on an overview page", () => {
  const { column } = mountColumn(
    "overview/demo/product",
    pageOf("overview/demo/product", [], {
      title: "Overview",
      blocks: [{ kind: "heading", level: 1, text: "Overview" }, body],
    }),
    false,
  );

  expect(
    within(column)
      .getAllByRole("heading")
      .map((heading) => heading.textContent),
  ).toEqual(["Overview"]);
});
