import { cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import type { TreeEntry } from "@/plan/api/schema-files";
import { TreeEntryRow } from "@/plan/files/tree-entry";
import { openQuestionsAtom } from "@/plan/review/live";

afterEach(cleanup);

it("keeps the question link after the title on the name's line, the row never wrapping, the link on one line", () => {
  const entry: TreeEntry = {
    feature: "loom",
    misplaced: false,
    path: "doc/plans/loom/METAPLAN.md",
    target: null,
    title: "loom plan Engine redesign",
    file_name: "METAPLAN.md",
    kind: "metaplan",
    state: null,
    locked: false,
    gap: false,
    pressure_date: null,
    errors: 0,
    blurb: null,
    children: [],
  };
  const store = createStore();
  store.set(openQuestionsAtom, { "doc/plans/loom/METAPLAN.md": 1 });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ul>
          <TreeEntryRow entry={entry} forceOpen={false} />
        </ul>
      </MemoryRouter>
    </Provider>,
  );

  const link = screen.getByRole("link", { name: "Open questions · 1" });
  expect(link.getAttribute("href")).toBe("/files/plans/loom/METAPLAN.md#open-questions");
  const title = screen.getByRole("link", { name: entry.title });
  const extras = link.parentElement;
  const row = title.parentElement;
  expect(row).toBe(extras?.parentElement);
  expect(row?.classList.contains("flex-nowrap")).toBe(true);
  expect(row?.classList.contains("flex-wrap")).toBe(false);
  expect(row?.classList.contains("min-w-0")).toBe(true);
  expect(row?.firstElementChild).toBe(title);
  expect(row?.lastElementChild).toBe(extras);
  expect(title.classList.contains("max-w-full")).toBe(true);
  for (const className of [
    "inline-flex",
    "flex-wrap",
    "[&_a]:whitespace-nowrap",
    "[&_a]:text-xs",
  ]) {
    expect(extras?.classList.contains(className)).toBe(true);
  }
  expect(extras?.classList.contains("shrink-0")).toBe(false);
});
