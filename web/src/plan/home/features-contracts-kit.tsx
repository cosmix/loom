import { render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { createMemoryRouter, MemoryRouter, RouterProvider } from "react-router";

import {
  projectChecklistViewSchema,
  type ChecklistView,
  type ProjectChecklistView,
  type StandalonePlanView,
} from "@/plan/api/schema";
import { FeatureRoute } from "@/plan/feature/feature-route";
import { projectAtom } from "@/plan/state/atoms";

const STEP_NAMES = ["Vision", "Product spec", "Metaplan", "Plans", "Pressure", "Review", "Run"];

function section(number: number) {
  return {
    number: String(number).padStart(2, "0"),
    name: `Section ${number}`,
    written: number % 2 === 0,
    open_questions: 0,
  };
}

/** A seven-step checklist whose current step is `current`; earlier steps are done. */
export function checklist(
  feature: string,
  {
    title,
    sections = 0,
    plans = 0,
    current = 2,
  }: Partial<Record<"sections" | "plans" | "current", number>> & {
    title: string;
  },
): ChecklistView {
  return {
    feature,
    pitch: null,
    title,
    has_product: true,
    sections: Array.from({ length: sections }, (_, index) => section(index + 1)),
    plans: { listed: plans, total: plans, written: plans, running: 0, done: 0 },
    untested: 0,
    current: { kind: "step", number: current },
    steps: STEP_NAMES.map((name, index) => ({
      number: index + 1,
      name,
      mark: index + 1 < current ? "done" : index + 1 === current ? "active" : "gap",
      count: null,
      label: name,
    })),
    primary: null,
    secondary: [],
    items: [],
  };
}

export function standalonePlans(count: number): StandalonePlanView[] {
  return Array.from({ length: count }, (_, index) => ({
    slug: `plan-${index + 1}`,
    path: `doc/plans/PLAN-plan-${index + 1}.md`,
    title: `Plan ${index + 1}`,
    state: "written",
  }));
}

/** Parses the project through the wire schema so contract data cannot drift from it. */
export function project(value: Partial<ProjectChecklistView>): ProjectChecklistView {
  return projectChecklistViewSchema.parse({ checklists: [], standalone: [], moves: [], ...value });
}

export function renderWithProject(ui: ReactNode, value: ProjectChecklistView) {
  const store = createStore();
  store.set(projectAtom, value);
  return render(
    <Provider store={store}>
      <MemoryRouter>{ui}</MemoryRouter>
    </Provider>,
  );
}

export function renderFeatureRoute(path: string, value: ProjectChecklistView) {
  const store = createStore();
  store.set(projectAtom, value);
  const router = createMemoryRouter(
    [
      { path: "/", element: <p>Home</p> },
      { path: "/f/:feature", element: <FeatureRoute /> },
    ],
    { initialEntries: [path] },
  );
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

/** Elements whose whole text, whitespace collapsed, is `text` and that hold no such child. */
export function elementsWithText(container: HTMLElement, text: string): Element[] {
  const normalize = (value: string | null) => (value ?? "").replace(/\s+/g, " ").trim();
  return Array.from(container.querySelectorAll("*")).filter(
    (element) =>
      normalize(element.textContent) === text &&
      !Array.from(element.children).some((child) => normalize(child.textContent) === text),
  );
}
