import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { ChecklistView } from "@/plan/api/schema";
import { FeatureCard, FeatureCards } from "@/plan/home/feature-card";
import { projectAtom } from "@/plan/state/atoms";

afterEach(cleanup);

function checklist(feature = "demo"): ChecklistView {
  return {
    feature,
    pitch: null,
    title: "Demo",
    has_product: true,
    sections: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    untested: 0,
    current: { kind: "step", number: 3 },
    steps: [
      { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
      { number: 2, name: "Product spec", mark: "done", count: null, label: "Product spec" },
      { number: 3, name: "Metaplan", mark: "gap", count: null, label: "Metaplan" },
      { number: 4, name: "Plans", mark: "active", count: null, label: "Plans" },
      { number: 5, name: "Pressure", mark: "optional", count: null, label: "Pressure" },
      { number: 6, name: "Review", mark: "optional", count: null, label: "Review" },
      { number: 7, name: "Run", mark: "optional", count: null, label: "Run" },
    ],
    primary: null,
    secondary: [],
    items: [
      { step: 3, text: "Metaplan missing", path: null, offer: null },
      { step: 3, text: "Metaplan decision missing", path: null, offer: null },
    ],
  };
}

function renderHome(ui: ReactNode, { features = [] }: { features?: ChecklistView[] } = {}) {
  const store = createStore();
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
      steps: [{ number: 1, name: "Vision", mark: "done", count: null, label: "Vision" }],
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

  it("shows the checklist pitch in the reading face, keeping chrome separate", () => {
    renderHome(
      <FeatureCard checklist={{ ...checklist(), pitch: "Make planning easier to follow." }} />,
    );

    const pitch = screen.getByText("Make planning easier to follow.").closest("p");
    expect(pitch?.classList.contains("feature-pitch")).toBe(true);
    expect(pitch?.parentElement?.closest(".feature-pitch")).toBeNull();
    expect(screen.getByRole("heading", { name: "demo" }).closest(".feature-pitch")).toBeNull();
    expect(screen.getByRole("list", { name: "Progress" }).closest(".feature-pitch")).toBeNull();
    expect(screen.getByText("2 of 7 steps done").closest(".feature-pitch")).toBeNull();
    expect(screen.getByText("2 gaps").closest(".feature-pitch")).toBeNull();
  });

  it("renders inline Markdown in the checklist pitch", () => {
    renderHome(
      <FeatureCard checklist={{ ...checklist(), pitch: "Build **clear** plans with `loom`." }} />,
    );

    const pitch = screen.getByText("clear").closest("p");
    expect(pitch?.querySelector("strong")?.textContent).toBe("clear");
    expect(pitch?.querySelector("code")?.textContent).toBe("loom");
    expect(pitch?.textContent).toBe("Build clear plans with loom.");
    expect(pitch?.classList.contains("feature-pitch")).toBe(true);
  });

  it.each([null, ""])("omits the paragraph when the checklist pitch is %s", (pitch) => {
    const { container } = renderHome(<FeatureCard checklist={{ ...checklist(), pitch }} />);

    expect(container.querySelector("p")).toBeNull();
  });

  it("escapes control characters in feature names and vision pitches", () => {
    renderHome(
      <FeatureCard checklist={{ ...checklist("demo\u202E"), pitch: "Build\u2066 with care." }} />,
    );

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
