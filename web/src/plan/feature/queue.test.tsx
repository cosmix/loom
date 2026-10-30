import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { Queue, queueRows, remainingItems } from "@/plan/feature/queue";

afterEach(cleanup);

const { type: _type, ...projectFrame } = checklistFixture;
const base: ChecklistView = projectChecklistViewSchema.parse(projectFrame).checklists[0]!;

const offer = {
  id: "write-plan:demo-01",
  kind: "write_plan" as const,
  label: "Write plan demo-01",
  target: null,
  feature: "demo",
};

const missing = {
  step: 4,
  text: "plan demo-01 named in doc/plans/demo/index.md is not written",
  path: "doc/plans/demo/PLAN-demo-01.md",
  offer,
};
const written = {
  step: 4,
  text: "plan demo-02",
  path: "doc/plans/demo/PLAN-demo-02.md",
  offer: null,
};
const unlisted = {
  step: 4,
  text: "plan demo-03 is not in a metaplan",
  path: "doc/plans/demo/PLAN-demo-03.md",
  offer: null,
};

function withItems(items: ChecklistView["items"]): ChecklistView {
  return { ...base, items };
}

describe("plan queue", () => {
  it("carries missing and written plans but leaves a plan outside a metaplan to the item list", () => {
    const checklist = withItems([missing, written, unlisted]);

    const rows = queueRows(checklist, "plan");
    expect(rows.map((row) => [row.name, row.written])).toEqual([
      ["demo-01", false],
      ["demo-02", true],
    ]);
    expect(remainingItems(checklist, 4)).toEqual([unlisted]);
  });

  it("does not show a plan outside a metaplan as written", () => {
    render(
      <Provider store={createStore()}>
        <MemoryRouter>
          <Queue checklist={withItems([written, unlisted])} kind="plan" />
        </MemoryRouter>
      </Provider>,
    );

    const rows = within(screen.getByRole("list", { name: "Plan queue" })).getAllByRole("listitem");
    expect(rows).toHaveLength(1);
    expect(screen.queryByText("demo-03")).toBeNull();
  });
});

describe("section queue", () => {
  it("counts open questions in the right number and renders backticked names as code", () => {
    const checklist: ChecklistView = {
      ...base,
      sections: [
        { number: "01", name: "The `Run` loop", written: true, open_questions: 1 },
        { number: "02", name: "Other", written: true, open_questions: 2 },
      ],
      items: [],
    };
    render(
      <Provider store={createStore()}>
        <MemoryRouter>
          <Queue checklist={checklist} kind="section" />
        </MemoryRouter>
      </Provider>,
    );

    expect(screen.getByText("1 open question")).toBeTruthy();
    expect(screen.getByText("2 open questions")).toBeTruthy();
    expect(screen.getByText("Run").tagName).toBe("CODE");
    expect(screen.queryByText(/`/)).toBeNull();
  });
});
