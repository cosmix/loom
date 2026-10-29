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

describe("FeatureRoute", () => {
  it("shows loading until the project arrives and then selects the feature", async () => {
    const { store } = renderRoute("/f/demo", null);

    expect(screen.getByRole("status", { name: "Loading the feature" })).toBeTruthy();
    expect(screen.queryByText("No such feature")).toBeNull();
    expect(store.get(activeFeatureAtom)).toBeNull();

    act(() => store.set(projectAtom, project));

    expect(screen.getByRole("heading", { level: 1, name: "demo" })).toBeTruthy();
    await waitFor(() => expect(store.get(activeFeatureAtom)).toBe("demo"));
  });

  it("shows an unknown feature and a link home without changing the active feature", () => {
    const { store } = renderRoute("/f/missing%E2%80%AEfeature", project, "demo");

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("missing⟨U+202E⟩feature")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Back to home" }).getAttribute("href")).toBe("/");
    expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
    expect(store.get(activeFeatureAtom)).toBe("demo");
  });

  it("renders the feature title, current step and step list", () => {
    renderRoute();

    expect(screen.getByRole("heading", { level: 1, name: "demo" })).toBeTruthy();
    expect(screen.getByText("Step 2 of 2: Product spec")).toBeTruthy();
    const steps = within(screen.getByRole("list", { name: "Steps" }));
    expect(steps.getAllByRole("heading").map((node) => node.textContent)).toEqual([
      "Conversation",
      "Vision",
      "Product spec",
    ]);
    expect(steps.getAllByRole("listitem", { current: "step" })).toHaveLength(1);
    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
  });

  it("renders each gap's offer once on its own row, grouped by step", () => {
    renderRoute("/f/demo", {
      ...project,
      checklists: [
        {
          ...checklist,
          steps: [...checklist.steps, { number: 3, name: "Metaplan", mark: "gap" }],
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
    const gaps = within(screen.getByRole("region", { name: "Gaps" }));
    const product = within(gaps.getByRole("region", { name: "Step 2 · Product spec" }));
    const sectionRow = within(product.getByRole("listitem"));
    expect(sectionRow.getByText("section 05: Example")).toBeTruthy();
    expect(sectionRow.getByText("doc/product/demo/section_05.md")).toBeTruthy();
    expect(sectionRow.getByText("Write section 05")).toBeTruthy();
    const metaplan = within(gaps.getByRole("region", { name: "Step 3 · Metaplan" }));
    const metaplanRow = within(metaplan.getByRole("listitem"));
    expect(metaplanRow.getByText("no metaplan")).toBeTruthy();
    expect(metaplanRow.getByText("Start a metaplan from the product spec")).toBeTruthy();
    expect(screen.getAllByText("Write section 05")).toHaveLength(1);
    expect(screen.getAllByText("Start a metaplan from the product spec")).toHaveLength(1);
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
      steps: [{ number: 1, name: "Other step", mark: "done" as const }],
      primary: null,
      secondary: [],
      items: [],
    };
    const value = { ...project, checklists: [other, checklist] };
    const { store } = renderRoute("/f/demo", value, "other");

    expect(screen.getByRole("heading", { level: 1, name: "demo" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Other step" })).toBeNull();
    expect(screen.getByRole("region", { name: "Gaps" })).toBeTruthy();
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

    expect(screen.getByRole("heading", { level: 1, name: "other" })).toBeTruthy();
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

    expect(screen.getByRole("heading", { level: 1, name: "other" })).toBeTruthy();
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

    expect(screen.getByRole("heading", { level: 1, name: "demo" })).toBeTruthy();
    expect(screen.getAllByText(label)).toHaveLength(2);
    expect(screen.queryByRole("listitem", { current: "step" })).toBeNull();
  });

  it.each(["a feature", "100% ready", "literal%20name", "de\u202Emo"])(
    "handles the encoded feature name %s without decoding literal escapes twice",
    (feature) => {
      const { store } = renderRoute(`/f/${encodeURIComponent(feature)}`, {
        ...project,
        checklists: [{ ...checklist, feature }],
      });

      expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
        feature.replace("\u202E", "⟨U+202E⟩"),
      );
      expect(store.get(activeFeatureAtom)).toBe(feature);
    },
  );

  it("handles an invalid percent escape without throwing", () => {
    renderRoute("/f/bad%25name");

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("bad%name")).toBeTruthy();
  });
});
