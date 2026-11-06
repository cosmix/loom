import { cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import type { TreeEntry } from "@/plan/api/schema-files";
import { TreeEntryRow } from "@/plan/files/tree-entry";
import { openQuestionsAtom } from "@/plan/review/live";

afterEach(cleanup);

it("sets the question count at the row's end, after the title, with no underline or wrap", () => {
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
  expect(link.getAttribute("title")).toBe("Open questions · 1");
  expect(link.classList.contains("plan-tree__questions")).toBe(true);
  // The words are read, not shown: the row keeps its width for the title.
  expect(link.querySelector(".sr-only")?.textContent).toBe("Open questions ·");
  const title = screen.getByRole("link", { name: "Metaplan" });
  const row = title.parentElement as HTMLElement;
  const signals = link.parentElement as HTMLElement;
  expect(signals.classList.contains("plan-tree__signals")).toBe(true);
  expect(signals.parentElement).toBe(row);
  expect([...row.children].indexOf(title)).toBeLessThan([...row.children].indexOf(signals));
});

it("draws no question count for a document whose questions are all answered", () => {
  const store = createStore();
  store.set(openQuestionsAtom, { "doc/notes.md": 0 });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ul>
          <TreeEntryRow
            entry={{
              feature: null,
              misplaced: false,
              path: "doc/notes.md",
              target: null,
              title: "Notes",
              file_name: "notes.md",
              kind: "other",
              state: null,
              locked: false,
              gap: false,
              pressure_date: null,
              errors: 0,
              blurb: null,
              children: [],
            }}
            forceOpen={false}
          />
        </ul>
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.queryByRole("link", { name: /open questions/i })).toBeNull();
});
