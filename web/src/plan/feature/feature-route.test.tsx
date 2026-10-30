import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema, type ProjectChecklistView } from "@/plan/api/schema";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { snapshotAtom } from "@/plan/chat/model";
import { FeatureRoute } from "@/plan/feature/feature-route";
import { activeFeatureAtom, checklistAtom, draftTeamAtom, projectAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const { type: _type, ...projectFrame } = checklistFixture;
const project = projectChecklistViewSchema.parse(projectFrame);
const checklist = project.checklists[0]!;

function renderRoute(
  path = "/f/demo",
  value: ProjectChecklistView | null = project,
  active: string | null = null,
) {
  const store = createStore();
  store.set(projectAtom, value);
  store.set(activeFeatureAtom, active);
  const router = createMemoryRouter(
    [
      { path: "/", element: <p>Home</p> },
      { path: "/f/:feature", element: <FeatureRoute /> },
    ],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, router };
}

function eyebrow(): string | undefined {
  return document.querySelector(".plan-eyebrow")?.textContent?.replace(/\s+/g, " ");
}

describe("FeatureRoute", () => {
  it("shows loading until the project arrives and then selects the feature", async () => {
    const { store } = renderRoute("/f/demo", null);

    expect(screen.getByRole("status", { name: "Loading the feature" })).toBeTruthy();
    expect(screen.queryByText("No such feature")).toBeNull();
    expect(store.get(activeFeatureAtom)).toBeNull();

    act(() => store.set(projectAtom, project));

    expect(screen.getByRole("heading", { level: 1, name: "Write the product spec" })).toBeTruthy();
    await waitFor(() => expect(store.get(activeFeatureAtom)).toBe("demo"));
  });

  it("shows an unknown feature and a link home without changing the active feature", () => {
    const { store } = renderRoute("/f/missing%E2%80%AEfeature", project, "demo");

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("missing⟨U+202E⟩feature")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Back to home" }).getAttribute("href")).toBe("/");
    expect(screen.queryByRole("navigation", { name: "Steps" })).toBeNull();
    expect(store.get(activeFeatureAtom)).toBe("demo");
  });

  it("renders the feature title, current step and step navigation", () => {
    renderRoute();

    expect(screen.getAllByRole("heading", { level: 1 }).map((node) => node.textContent)).toEqual([
      "Write the product spec",
    ]);
    expect(eyebrow()).toBe("Demo · step 2 of 7");
    const steps = within(screen.getByRole("navigation", { name: "Steps" }));
    expect(steps.getAllByRole("link").map((node) => node.textContent?.split(",")[0])).toEqual([
      "Conversation",
      "Vision",
      "Product spec",
    ]);
    expect(steps.getAllByRole("link", { current: "step" })).toHaveLength(1);
    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
  });

  it("selects the step from the query and falls back to the current step", () => {
    renderRoute("/f/demo?step=1");
    expect(screen.getByRole("heading", { level: 1, name: "Write the vision" })).toBeTruthy();
    expect(eyebrow()).toBe("Demo · step 1 of 7");
    cleanup();

    renderRoute("/f/demo?step=9");
    expect(screen.getByRole("heading", { level: 1, name: "Write the product spec" })).toBeTruthy();
  });

  it("lists the sections as a queue on the product spec step", () => {
    renderRoute();

    const queue = within(screen.getByRole("list", { name: "Section queue" }));
    const row = within(queue.getByRole("listitem"));
    expect(row.getByText("05")).toBeTruthy();
    expect(row.getByText("Example")).toBeTruthy();
    expect(row.getByText("Missing")).toBeTruthy();
  });

  it("shows the pace choice in the body of the selected step only", () => {
    const guided = {
      id: "write-section:05",
      kind: "write_section" as const,
      label: "Write section 05",
      target: null,
      feature: "demo",
      number: 5,
    };
    const team = {
      id: "team",
      kind: "draft_all_sections" as const,
      label: "Draft all",
      target: null,
      feature: "demo",
    };
    renderRoute("/f/demo", {
      ...project,
      checklists: [
        {
          ...checklist,
          primary: guided,
          secondary: [team],
          items: [{ step: 2, text: "Missing section", path: null, offer: guided }],
        },
      ],
    });

    expect(screen.getAllByRole("heading", { name: "Guided, one section at a time" })).toHaveLength(
      1,
    );
    cleanup();
    renderRoute("/f/demo?step=3", {
      ...project,
      checklists: [{ ...checklist, primary: guided, secondary: [team], items: [] }],
    });
    expect(screen.queryByRole("heading", { name: "Guided, one section at a time" })).toBeNull();
  });

  it("puts a section's offer on its queue row and drops the repeated item list", () => {
    renderRoute("/f/demo", {
      ...project,
      checklists: [
        {
          ...checklist,
          steps: [
            ...checklist.steps,
            { number: 3, name: "Metaplan", mark: "gap", count: null, label: "Metaplan" },
          ],
          items: [
            ...checklist.items,
            {
              step: 3,
              text: "no metaplan",
              path: null,
              offer: checklist.secondary[0] ?? null,
            },
          ],
        },
      ],
    });

    expect(screen.queryByRole("region", { name: "Next" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 2 })).toBeNull();
    expect(screen.queryByText(/^Step \d$/)).toBeNull();
    expect(screen.queryByText("section 05: Example")).toBeNull();
    expect(screen.queryByRole("list", { name: "Documents and gaps" })).toBeNull();
    const row = within(
      within(screen.getByRole("list", { name: "Section queue" })).getByRole("listitem"),
    );
    expect(row.getByText("Missing")).toBeTruthy();
    expect(row.getByText("Write section 05")).toBeTruthy();
    expect(screen.getAllByText("Write section 05")).toHaveLength(1);
    expect(screen.queryByText("no metaplan")).toBeNull();
  });

  it("names plan queue rows by slug and puts each offer on its row", () => {
    const offer = {
      id: "write-plan:demo-01",
      kind: "write_plan" as const,
      label: "Write plan demo-01",
      target: null,
      feature: "demo",
    };
    renderRoute("/f/demo?step=4", {
      ...project,
      checklists: [
        {
          ...checklist,
          items: [
            {
              step: 4,
              text: "plan demo-01 named in doc/plans/demo/index.md is not written",
              path: "doc/plans/demo/PLAN-demo-01.md",
              offer,
            },
            {
              step: 4,
              text: "plan demo-02",
              path: "doc/plans/demo/PLAN-demo-02.md",
              offer: null,
            },
          ],
        },
      ],
    });

    const queue = within(screen.getByRole("list", { name: "Plan queue" }));
    const [missing, written] = queue.getAllByRole("listitem").map((row) => within(row));
    expect(missing!.getByText("demo-01")).toBeTruthy();
    expect(missing!.getByText("Write plan demo-01")).toBeTruthy();
    expect(written!.getByRole("link", { name: "demo-02" }).getAttribute("href")).toBe(
      "/files/plans/demo/PLAN-demo-02.md",
    );
    expect(screen.queryByText(/is not written/)).toBeNull();
    expect(screen.queryByRole("list", { name: "Documents and gaps" })).toBeNull();
  });

  it("renders the metaplan gap's offer once when that step is selected", () => {
    renderRoute("/f/demo?step=3", {
      ...project,
      checklists: [
        {
          ...checklist,
          steps: [
            ...checklist.steps,
            { number: 3, name: "Metaplan", mark: "gap", count: null, label: "Metaplan" },
          ],
          items: [
            ...checklist.items,
            {
              step: 3,
              text: "no metaplan",
              path: null,
              offer: checklist.secondary[0] ?? null,
            },
          ],
        },
      ],
    });

    expect(screen.queryByRole("heading", { level: 2 })).toBeNull();
    const gaps = within(screen.getByRole("list", { name: "Documents and gaps" }));
    const metaplanRow = within(gaps.getByRole("listitem"));
    expect(metaplanRow.getByText("no metaplan")).toBeTruthy();
    expect(metaplanRow.getByText("Start a metaplan from the product spec")).toBeTruthy();
    expect(screen.getAllByText("Start a metaplan from the product spec")).toHaveLength(1);
    expect(screen.queryByText("Write section 05")).toBeNull();
  });

  it("leaves a primary offer without a gap row to the header card", () => {
    renderRoute("/f/demo", {
      ...project,
      checklists: [
        {
          ...checklist,
          current: { kind: "ready" },
          primary: { id: "start-run", kind: "start_run", label: "Start run", target: null },
          secondary: [],
          items: [],
        },
      ],
    });

    expect(screen.queryByText("Start run")).toBeNull();
    expect(screen.queryByRole("region", { name: "Next" })).toBeNull();
  });

  it("uses the requested feature and synchronizes the active checklist after render", async () => {
    const other = {
      ...checklist,
      feature: "other",
      steps: [
        { number: 1, name: "Other step", mark: "done" as const, count: null, label: "Other step" },
      ],
      primary: null,
      secondary: [],
      items: [],
    };
    const value = { ...project, checklists: [other, checklist] };
    const { store } = renderRoute("/f/demo", value, "other");

    expect(eyebrow()).toBe("Demo · step 2 of 7");
    expect(screen.queryByRole("link", { name: /Other step/ })).toBeNull();
    expect(screen.getByRole("list", { name: "Section queue" })).toBeTruthy();
    await waitFor(() => expect(store.get(activeFeatureAtom)).toBe("demo"));
    expect(store.get(checklistAtom)).toEqual(checklist);
  });

  it("updates the active feature when navigating between feature screens", async () => {
    const other = { ...checklist, feature: "other", primary: null, secondary: [], items: [] };
    const { store, router } = renderRoute("/f/demo", {
      ...project,
      checklists: [checklist, other],
    });

    await act(async () => {
      await router.navigate("/f/other");
    });

    expect(screen.getByRole("link", { name: /^Conversation, done other$/ })).toBeTruthy();
    expect(store.get(activeFeatureAtom)).toBe("other");
    expect(screen.queryByRole("region", { name: "Gaps" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Next" })).toBeNull();
  });

  it("shows an active team and Stop only on the feature owning its documents", async () => {
    const other = { ...checklist, feature: "other", primary: null, secondary: [], items: [] };
    const { store, router } = renderRoute("/f/demo", {
      ...project,
      checklists: [checklist, other],
    });
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
    expect(screen.getByRole("region", { name: "Draft team" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop the team" })).toBeTruthy();

    await act(async () => {
      await router.navigate("/f/other");
    });

    expect(screen.getByRole("link", { name: /^Conversation, done other$/ })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });

  it.each([
    ["ready", "Ready to run"],
    ["ready_all_done", "All plans done"],
  ] as const)("renders the heading for the %s state", (kind, label) => {
    renderRoute("/f/demo", {
      ...project,
      checklists: [{ ...checklist, current: { kind } }],
    });

    expect(screen.getByRole("heading", { level: 1, name: "Run the plans" })).toBeTruthy();
    expect(eyebrow()).toBe("Demo · step 7 of 7");
    expect(screen.getAllByText(label)).toHaveLength(1);
    expect(screen.queryByRole("link", { current: "step" })).toBeNull();
  });

  it.each(["a feature", "100% ready", "literal%20name", "de\u202Emo"])(
    "handles the encoded feature name %s without decoding literal escapes twice",
    (feature) => {
      const { store } = renderRoute(`/f/${encodeURIComponent(feature)}`, {
        ...project,
        checklists: [{ ...checklist, feature }],
      });

      expect(
        within(screen.getByRole("navigation", { name: "Steps" })).getAllByRole("link")[0]
          ?.textContent,
      ).toBe(`Conversation, done ${feature.replace("\u202E", "⟨U+202E⟩")}`);
      expect(store.get(activeFeatureAtom)).toBe(feature);
    },
  );

  it("handles an invalid percent escape without throwing", () => {
    renderRoute("/f/bad%25name");

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("bad%name")).toBeTruthy();
  });
});
