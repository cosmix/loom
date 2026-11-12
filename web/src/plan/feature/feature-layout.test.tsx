import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import {
  checklistViewSchema,
  type ChecklistView,
  type ProjectChecklistView,
} from "@/plan/api/schema";
import { FeatureLanding, FeatureLayout } from "@/plan/feature/feature-layout";
import { activeFeatureAtom, projectAtom } from "@/plan/state/atoms";

afterEach(cleanup);

function named(feature: string, patch: Record<string, unknown> = {}): ChecklistView {
  return checklistViewSchema.parse({ ...checklistFixture.checklists[0]!, feature, ...patch });
}

function projectOf(...checklists: ChecklistView[]): ProjectChecklistView {
  return { checklists, standalone: [], moves: [] };
}

function renderLayout(
  path: string,
  value: ProjectChecklistView | null,
  active: string | null = null,
) {
  const store = createStore();
  store.set(projectAtom, value);
  store.set(activeFeatureAtom, active);
  const router = createMemoryRouter(
    [
      { path: "/", element: <p>Home</p> },
      {
        path: "/f/:feature",
        element: <FeatureLayout />,
        children: [
          { index: true, element: <FeatureLanding /> },
          { path: "*", element: <p>Page body</p> },
        ],
      },
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

describe("FeatureLayout", () => {
  it("shows loading until the project arrives, then selects the feature and lands", async () => {
    const { store, router } = renderLayout("/f/demo", null);

    expect(screen.getByRole("status", { name: "Loading the feature" })).toBeTruthy();
    expect(screen.queryByText("No such feature")).toBeNull();
    expect(store.get(activeFeatureAtom)).toBeNull();

    act(() => store.set(projectAtom, projectOf(named("demo"))));

    await waitFor(() => expect(store.get(activeFeatureAtom)).toBe("demo"));
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/product"));
    expect(router.state.historyAction).toBe("REPLACE");
    expect(screen.getByText("Page body")).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Feature documents" })).toBeTruthy();
  });

  it("shows an unknown feature and a link home without changing the active feature", () => {
    const { store } = renderLayout("/f/missing%E2%80%AEfeature", projectOf(named("demo")), "demo");

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("missing⟨U+202E⟩feature")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Back to home" }).getAttribute("href")).toBe("/");
    expect(screen.queryByRole("navigation", { name: "Feature documents" })).toBeNull();
    expect(store.get(activeFeatureAtom)).toBe("demo");
  });

  it("draws the routed page without a spine while the project is loading", () => {
    renderLayout("/f/demo/anything", null);

    expect(screen.getByText("Page body")).toBeTruthy();
    expect(screen.queryByRole("navigation", { name: "Feature documents" })).toBeNull();
  });

  it("uses the requested feature, not the active one, and synchronizes after render", async () => {
    const { store } = renderLayout(
      "/f/demo/vision",
      projectOf(named("other"), named("demo")),
      "other",
    );

    const spine = screen.getByRole("navigation", { name: "Feature documents" });
    expect(
      within(spine)
        .getByRole("link", { name: /\bVision\b/ })
        .getAttribute("href"),
    ).toBe("/f/demo/vision");
    await waitFor(() => expect(store.get(activeFeatureAtom)).toBe("demo"));
  });

  it("updates the active feature when navigating between features", async () => {
    const { store, router } = renderLayout(
      "/f/demo/vision",
      projectOf(named("demo"), named("other")),
    );

    await act(async () => {
      await router.navigate("/f/other/vision");
    });

    expect(store.get(activeFeatureAtom)).toBe("other");
    const spine = screen.getByRole("navigation", { name: "Feature documents" });
    expect(
      within(spine)
        .getByRole("link", { name: /\bVision\b/ })
        .getAttribute("href"),
    ).toBe("/f/other/vision");
  });

  it.each(["a feature", "100% ready", "literal%20name", "de‮mo"])(
    "handles the encoded feature name %s without decoding literal escapes twice",
    (feature) => {
      const { store } = renderLayout(
        `/f/${encodeURIComponent(feature)}/vision`,
        projectOf(named(feature)),
      );

      expect(screen.getByRole("navigation", { name: "Feature documents" })).toBeTruthy();
      expect(store.get(activeFeatureAtom)).toBe(feature);
    },
  );

  it("handles an invalid percent escape without throwing", () => {
    renderLayout("/f/bad%25name", projectOf(named("demo")));

    expect(screen.getByText("No such feature")).toBeTruthy();
    expect(screen.getByText("bad%name")).toBeTruthy();
  });

  it("keeps the spine's column while the project loads, so the page does not move when it joins", () => {
    const { store } = renderLayout("/f/demo/anything", null);
    const layout = document.querySelector(".plan-layout")!;
    const page = layout.querySelector(".plan-layout__page")!;
    const slot = layout.querySelector(".plan-layout__spine-slot");

    expect(layout.hasAttribute("data-spine")).toBe(true);
    expect(slot?.nextElementSibling).toBe(page);
    expect(slot?.getAttribute("aria-hidden")).toBe("true");

    act(() => store.set(projectAtom, projectOf(named("demo"))));

    const spine = screen.getByRole("navigation", { name: "Feature documents" });
    expect(layout.querySelector(".plan-layout__spine-slot")).toBeNull();
    expect(spine.nextElementSibling).toBe(page);
  });
});
