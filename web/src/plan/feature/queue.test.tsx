import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { hasGaps, Queue, queueRows } from "@/plan/feature/queue";
import { registerOfferAction } from "@/plan/offers/actions";
import { authoringAtom, draftTeamAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const { type: _type, ...projectFrame } = checklistFixture;
const base: ChecklistView = projectChecklistViewSchema.parse(projectFrame).checklists[0]!;

const planRow = (slug: string, state: string, listed = true) => ({
  slug,
  path: `doc/plans/demo/PLAN-${slug}.md`,
  title: `Title of ${slug}`,
  state,
  pressure_date: null,
  listed,
});

const offer = {
  id: "write-plan:demo-01",
  kind: "write_plan" as const,
  label: "Write plan demo-01",
  target: null,
  feature: "demo",
  slug: "demo-01",
};

const missing = {
  step: 4,
  text: "plan demo-01 named in doc/plans/demo/index.md is not written",
  path: "doc/plans/demo/PLAN-demo-01.md",
  offer,
};

function renderQueue(checklist: ChecklistView, kind: "section" | "plan", store = createStore()) {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <Queue checklist={checklist} kind={kind} />
      </MemoryRouter>
    </Provider>,
  );
}

const rowsOf = (name: string) =>
  within(screen.getByRole("list", { name })).getAllByRole("listitem");

describe("plan queue", () => {
  const checklist = {
    ...base,
    plan_rows: [planRow("demo-02", "written"), planRow("demo-03", "done", false)],
    items: [missing],
  };

  it("carries written and missing plans but leaves a plan outside a metaplan out", () => {
    expect(queueRows(checklist, "plan").map((row) => [row.name, row.written])).toEqual([
      ["demo-02", true],
      ["demo-01", false],
    ]);
    expect(hasGaps(checklist, "plan")).toBe(true);
  });

  it("does not show a plan outside a metaplan, as written or otherwise", () => {
    renderQueue(checklist, "plan");

    expect(rowsOf("Plan queue")).toHaveLength(2);
    expect(screen.queryByText("demo-03")).toBeNull();
  });

  it("names rows by slug and links a written plan to its concept page", () => {
    renderQueue(checklist, "plan");

    const [written, gap] = rowsOf("Plan queue").map((row) => within(row));
    expect(written!.getByRole("link", { name: "demo-02" }).getAttribute("href")).toBe(
      "/f/demo/engineering/demo-02",
    );
    expect(written!.getByText("Written")).toBeTruthy();
    expect(gap!.getByText("demo-01")).toBeTruthy();
    expect(gap!.queryByRole("link")).toBeNull();
    expect(gap!.getByText("Missing")).toBeTruthy();
    expect(screen.queryByText(/is not written/)).toBeNull();
  });

  it("numbers a row by the number its slug carries, not by its place in the queue", () => {
    renderQueue(checklist, "plan");

    const numbers = rowsOf("Plan queue").map(
      (row) => row.querySelector(".plan-queue__num")?.textContent,
    );
    expect(numbers).toEqual(["02", "01"]);
  });

  it("shows Next and no button on the row of the offer the pace choice draws", () => {
    const unregister = registerOfferAction("write_plan", () => {});
    try {
      const team = {
        id: "draft-all-plans",
        kind: "draft_all_plans" as const,
        label: "Draft all plans",
        target: null,
        feature: "demo",
      };
      const other = {
        id: "start-run",
        kind: "start_run" as const,
        label: "Start a run",
        target: null,
      };
      renderQueue({ ...checklist, primary: other, secondary: [team] }, "plan");

      const [, gap] = rowsOf("Plan queue").map((row) => within(row));
      expect(gap!.getByText("Next")).toBeTruthy();
      expect(gap!.queryByRole("button")).toBeNull();
    } finally {
      unregister();
    }
  });

  it("draws a button on a missing row, and none while authoring is blocked", () => {
    const unregister = registerOfferAction("write_plan", () => {});
    try {
      renderQueue(checklist, "plan");
      expect(within(rowsOf("Plan queue")[1]!).getByRole("button")).toBeTruthy();
      cleanup();

      // The page's primary carries the reason once; a row repeats nothing.
      const blocked = createStore();
      const holder = { public_id: "sam", name: "Sam", away_since: null, frees_at: null };
      blocked.set(authoringAtom, { holder });
      renderQueue(checklist, "plan", blocked);
      expect(within(rowsOf("Plan queue")[1]!).queryByRole("button")).toBeNull();
      expect(screen.queryByText(/holds authoring/)).toBeNull();
    } finally {
      unregister();
    }
  });

  it("has no gaps once every listed plan is written", () => {
    expect(hasGaps({ ...checklist, items: [] }, "plan")).toBe(false);
  });
});

describe("section queue", () => {
  const sections = [
    { number: "01", name: "The `Run` loop", written: true, open_questions: 1 },
    { number: "02", name: "Other", written: false, open_questions: 2 },
  ];

  it("counts open questions in the right number and renders backticked names as code", () => {
    renderQueue({ ...base, sections, items: [] }, "section");

    expect(screen.getByText("1 open question")).toBeTruthy();
    expect(screen.getByText("2 open questions")).toBeTruthy();
    expect(screen.getByText("Run").tagName).toBe("CODE");
    expect(screen.queryByText(/`/)).toBeNull();
  });

  it("links a written section to its concept page and not a missing one", () => {
    renderQueue({ ...base, sections, items: [] }, "section");

    const [written, gap] = rowsOf("Section queue").map((row) => within(row));
    expect(written!.getByRole("link").getAttribute("href")).toBe("/f/demo/product/section_01");
    expect(gap!.queryByRole("link")).toBeNull();
    expect(hasGaps({ ...base, sections, items: [] }, "section")).toBe(true);
  });

  it("puts a section's offer on its row, once", () => {
    const unregister = registerOfferAction("write_section", () => {});
    try {
      renderQueue({ ...base, primary: null, secondary: [] }, "section");

      const [row] = rowsOf("Section queue").map((item) => within(item));
      expect(row!.getByRole("button", { name: "Write section 05" })).toBeTruthy();
      expect(screen.getAllByText("Write section 05")).toHaveLength(1);
      expect(screen.queryByText("section 05: Example")).toBeNull();
    } finally {
      unregister();
    }
  });

  it("shows Next and no button on the row of the primary offer", () => {
    const unregister = registerOfferAction("write_section", () => {});
    try {
      renderQueue(base, "section");

      const [row] = rowsOf("Section queue").map((item) => within(item));
      expect(row!.getByText("Next")).toBeTruthy();
      expect(row!.queryByRole("button")).toBeNull();
    } finally {
      unregister();
    }
  });

  it("marks a section a teammate is drafting", () => {
    const store = createStore();
    store.set(draftTeamAtom, {
      session: "draft",
      documents: ["doc/product/demo/section_05.md"],
      teammates: [
        {
          agent_id: "writer",
          name: "Ada",
          documents: ["doc/product/demo/section_05.md"],
          current: "doc/product/demo/section_05.md",
          state: "working",
          tokens: { input: 0, cache_write: 0, cache_read: 0, output: 0 },
        },
      ],
    });
    renderQueue(base, "section", store);

    const [row] = rowsOf("Section queue").map((item) => within(item));
    expect(row!.getByText("Drafting…")).toBeTruthy();
    expect(row!.queryByText("Missing")).toBeNull();
  });
});
