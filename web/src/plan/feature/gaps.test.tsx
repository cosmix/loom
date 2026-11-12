import { act, cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { snapshotAtom } from "@/plan/chat/model";
import { Gaps } from "@/plan/feature/gaps";
import type { GapScope } from "@/plan/feature/page-offer";
import { draftTeamAtom, projectAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const guided = {
  id: "write-section:05",
  kind: "write_section" as const,
  blocked_reason: null,
  label: "Write section 05",
  target: null,
  feature: "demo",
  number: 5,
};
const team = {
  id: "team",
  kind: "draft_all_sections" as const,
  blocked_reason: null,
  label: "Draft all",
  target: null,
  feature: "demo",
};

function demo(patch: Record<string, unknown> = {}): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    secondary: [team],
    items: [
      {
        step: 2,
        text: "section 05: Example",
        path: "doc/product/demo/section_05.md",
        offer: guided,
      },
    ],
    ...patch,
  });
}

function renderGaps(scope: GapScope, checklist: ChecklistView, feature = checklist.feature) {
  const store = createStore();
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <Gaps scope={scope} /> }], {
    initialEntries: [`/f/${feature}/${scope}`],
  });
  const view = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { ...view, store, router };
}

const pace = () => screen.queryByRole("group", { name: "Choose a pace" });

describe("product gaps", () => {
  it("draws the pace choice and the section queue while a section is missing", () => {
    renderGaps("product", demo());

    expect(pace()).toBeTruthy();
    expect(screen.getByRole("list", { name: "Section queue" })).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Plan queue" })).toBeNull();
    expect(screen.queryByRole("list", { name: "Documents and gaps" })).toBeNull();
  });

  it("draws nothing once every section is written and no pace is offered", () => {
    const written = demo({
      sections: [{ number: "05", name: "Example", written: true, open_questions: 0 }],
      items: [],
      secondary: [],
    });
    const { container } = renderGaps("product", written);

    expect(pace()).toBeNull();
    expect(screen.queryByRole("list", { name: "Section queue" })).toBeNull();
    expect(container.querySelector(".plan-gaps")?.childElementCount ?? 0).toBe(0);
  });

  it("shows no pace choice without a team offer, but still the queue", () => {
    renderGaps("product", demo({ secondary: [] }));

    expect(pace()).toBeNull();
    expect(screen.getByRole("list", { name: "Section queue" })).toBeTruthy();
  });

  it("offers each offer once: the pace card carries the primary, the queue row says Next", () => {
    renderGaps("product", demo());

    const row = within(screen.getByRole("list", { name: "Section queue" })).getByRole("listitem");
    expect(within(row).getByText("Next")).toBeTruthy();
    expect(within(row).queryByRole("button")).toBeNull();
  });
});

describe("metaplan gaps", () => {
  const writePlan = {
    id: "write-plan:demo-01",
    kind: "write_plan" as const,
    blocked_reason: null,
    label: "Write plan",
    target: null,
    feature: "demo",
    number: 1,
    slug: "demo-01",
  };
  const draftPlans = { ...team, id: "plans-team", kind: "draft_all_plans" as const };

  it("draws the pace choice and the plan queue while a plan is missing", () => {
    renderGaps(
      "metaplan",
      demo({
        primary: writePlan,
        secondary: [draftPlans],
        items: [
          {
            step: 4,
            text: "plan demo-01",
            path: "doc/plans/demo/PLAN-demo-01.md",
            offer: writePlan,
          },
        ],
      }),
    );

    expect(screen.getByRole("heading", { name: "Guided, one plan at a time" })).toBeTruthy();
    expect(screen.getByRole("list", { name: "Plan queue" })).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Section queue" })).toBeNull();
  });

  it("does not show the section pace on the plans page", () => {
    renderGaps("metaplan", demo());

    expect(pace()).toBeNull();
    expect(screen.queryByRole("list", { name: "Plan queue" })).toBeNull();
  });
});

describe("draft team", () => {
  function teamAt(store: ReturnType<typeof createStore>) {
    act(() => {
      store.set(snapshotAtom, {
        session: sessionViewSchema.parse({ ...snapshotFixture.session, id: "draft-session" }),
        turns: [],
        queue: [],
        previous: null,
      });
      store.set(draftTeamAtom, {
        session: "draft-session",
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
    });
  }

  it("shows an active team and Stop only on the feature owning its documents", () => {
    const { store } = renderGaps("product", demo());
    teamAt(store);

    expect(screen.getByRole("region", { name: "Draft team" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop the team" })).toBeTruthy();
    cleanup();

    const other = renderGaps("product", demo({ feature: "other", items: [], secondary: [] }));
    teamAt(other.store);

    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });
});
