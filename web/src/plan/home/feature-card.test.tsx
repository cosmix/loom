import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { ChecklistView } from "@/plan/api/schema";
import type { FileTree, TreeEntry } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { FeatureCard, FeatureCards } from "@/plan/home/feature-card";
import { projectAtom } from "@/plan/state/atoms";

afterEach(cleanup);

function checklist(feature = "demo"): ChecklistView {
  return {
    feature,
    current: { kind: "step", number: 3 },
    steps: [
      { number: 1, name: "Vision", mark: "done" },
      { number: 2, name: "Product spec", mark: "done" },
      { number: 3, name: "Metaplan", mark: "gap" },
      { number: 4, name: "Plans", mark: "active" },
      { number: 5, name: "Pressure", mark: "optional" },
      { number: 6, name: "Review", mark: "optional" },
      { number: 7, name: "Run", mark: "optional" },
    ],
    primary: null,
    secondary: [],
    items: [
      { step: 3, text: "Metaplan missing", path: null, offer: null },
      { step: 3, text: "Metaplan decision missing", path: null, offer: null },
    ],
  };
}

function entry(overrides: Partial<TreeEntry> = {}): TreeEntry {
  return {
    feature: "demo",
    misplaced: false,
    path: null,
    title: "demo",
    file_name: null,
    kind: "feature",
    state: null,
    locked: false,
    gap: false,
    pressure_date: null,
    errors: 0,
    blurb: null,
    children: [],
    ...overrides,
  };
}

function visionTree(blurb: string | null): FileTree {
  return {
    product: [
      entry({ path: "doc/product/other/index.md", blurb: "Another feature's pitch" }),
      entry({
        blurb: "The folder's blurb",
        children: [entry({ kind: "vision", path: "doc/product/demo/index.md", blurb })],
      }),
    ],
    plans: [],
    knowledge: [],
    other: [],
    archive: [],
  };
}

function renderHome(
  ui: ReactNode,
  { tree = null, features = [] }: { tree?: FileTree | null; features?: ChecklistView[] } = {},
) {
  const store = createStore();
  store.set(treeAtom, tree);
  store.set(projectAtom, { checklists: features, standalone: [], moves: [] });
  return render(
    <Provider store={store}>
      <MemoryRouter>{ui}</MemoryRouter>
    </Provider>,
  );
}

describe("FeatureCard", () => {
  it("shows the feature name and links to its encoded route", () => {
    renderHome(<FeatureCard checklist={checklist("demo / café")} />);

    expect(screen.getByRole("heading", { name: "demo / café" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "demo / café" }).getAttribute("href")).toBe(
      "/f/demo%20%2F%20caf%C3%A9",
    );
  });

  it("counts completed steps and checklist items as gaps", () => {
    renderHome(<FeatureCard checklist={checklist()} />);

    expect(screen.getByText("2 of 7 steps done")).toBeTruthy();
    expect(screen.getByText("2 gaps")).toBeTruthy();
    expect(screen.getByRole("list", { name: "Progress" })).toBeTruthy();
  });

  it("uses singular labels for one step and one gap", () => {
    const value: ChecklistView = {
      ...checklist(),
      steps: [{ number: 1, name: "Vision", mark: "done" }],
      items: [{ step: 1, text: "Vision decision missing", path: null, offer: null }],
    };
    renderHome(<FeatureCard checklist={value} />);

    expect(screen.getByText("1 of 1 step done")).toBeTruthy();
    expect(screen.getByText("1 gap")).toBeTruthy();
  });

  it("uses plural labels for zero counts", () => {
    renderHome(<FeatureCard checklist={{ ...checklist(), steps: [], items: [] }} />);

    expect(screen.getByText("0 of 0 steps done")).toBeTruthy();
    expect(screen.getByText("0 gaps")).toBeTruthy();
  });

  it("shows the pitch from the nested vision matching the feature path", () => {
    renderHome(<FeatureCard checklist={checklist()} />, {
      tree: visionTree("Make planning easier to follow."),
    });

    expect(screen.getByText("Make planning easier to follow.")).toBeTruthy();
    expect(screen.queryByText("Another feature's pitch")).toBeNull();
    expect(screen.queryByText("The folder's blurb")).toBeNull();
  });

  it.each([null, ""])("omits the pitch when the vision blurb is %s", (blurb) => {
    const { container } = renderHome(<FeatureCard checklist={checklist()} />, {
      tree: visionTree(blurb),
    });

    expect(container.querySelector("p")).toBeNull();
  });

  it("omits the pitch when the feature vision is absent", () => {
    const { container } = renderHome(<FeatureCard checklist={checklist("missing")} />, {
      tree: visionTree("Another vision's pitch"),
    });

    expect(container.querySelector("p")).toBeNull();
    expect(screen.queryByText("Another vision's pitch")).toBeNull();
  });

  it("omits the pitch before the files tree loads", () => {
    const { container } = renderHome(<FeatureCard checklist={checklist()} />);

    expect(container.querySelector("p")).toBeNull();
  });

  it("escapes control characters in feature names and vision pitches", () => {
    renderHome(<FeatureCard checklist={checklist("demo\u202E")} />, {
      tree: {
        ...visionTree(null),
        product: [
          entry({ path: "doc/product/demo\u202E/index.md", blurb: "Build\u2066 with care." }),
        ],
      },
    });

    expect(screen.getByRole("heading", { name: "demo⟨U+202E⟩" })).toBeTruthy();
    expect(screen.getByText("Build⟨U+2066⟩ with care.")).toBeTruthy();
  });
});

describe("FeatureCards", () => {
  it("renders a card for every feature from project state", () => {
    renderHome(<FeatureCards />, { features: [checklist("alpha"), checklist("beta")] });

    const list = screen.getByRole("list", { name: "Features" });
    expect(within(list).getAllByRole("link")).toHaveLength(2);
    expect(within(list).getByRole("link", { name: "alpha" }).getAttribute("href")).toBe("/f/alpha");
    expect(within(list).getByRole("link", { name: "beta" }).getAttribute("href")).toBe("/f/beta");
  });

  it("renders nothing when there are no features", () => {
    const { container } = renderHome(<FeatureCards />);

    expect(container.innerHTML).toBe("");
  });
});
