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
    has_product: true,
    sections: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    untested: 0,
    metaplans: [],
    plan_rows: [],
    current: { kind: "step", number: 3 },
    steps: [
      { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
      { number: 2, name: "Product spec", mark: "done", count: null, label: "Product spec" },
      { number: 3, name: "Metaplan", mark: "gap", count: null, label: "Metaplan" },
      { number: 4, name: "Plans", mark: "active", count: null, label: "Plans" },
      { number: 5, name: "Pressure", mark: "optional", count: null, label: "Pressure" },
      { number: 6, name: "Run", mark: "optional", count: null, label: "Run" },
    ],
    primary: null,
    secondary: [],
    items: [
      { step: 3, text: "Metaplan missing", path: null, offer: null },
      { step: 3, text: "Metaplan decision missing", path: null, offer: null },
    ],
  };
}

const store = createStore();

function wrap(ui: ReactNode) {
  return (
    <Provider store={store}>
      <MemoryRouter>{ui}</MemoryRouter>
    </Provider>
  );
}

function renderHome(ui: ReactNode, { features = [] }: { features?: ChecklistView[] } = {}) {
  store.set(projectAtom, { checklists: features, standalone: [], moves: [] });
  return render(wrap(ui));
}

describe("FeatureCard", () => {
  it("shows the slug and links to the encoded route", () => {
    renderHome(<FeatureCard checklist={checklist("demo / café")} />);

    expect(screen.getByRole("heading", { name: "demo / café" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "demo / café" }).getAttribute("href")).toBe(
      "/f/demo%20%2F%20caf%C3%A9",
    );
  });

  it("folds the step and gap counts into screen-reader text beside the progress cells", () => {
    renderHome(<FeatureCard checklist={checklist()} />);

    expect(screen.getByText("2 of 6 steps done, 1 with gaps")).toBeTruthy();
    expect(screen.queryByText("2 of 6 steps done")).toBeNull();
    expect(screen.getByRole("list", { name: "Progress" })).toBeTruthy();
  });

  it("describes the link with the folded step summary", () => {
    renderHome(<FeatureCard checklist={checklist()} />);

    const link = screen.getByRole("link", { name: "demo" });
    expect(link.getAttribute("aria-describedby")).toBeTruthy();
    expect(link.getAttribute("aria-describedby")).not.toBe(link.getAttribute("aria-labelledby"));
    expect(screen.getByRole("link", { name: "demo", description: /2 of 6 steps done/ })).toBe(link);
  });

  it("uses the singular label for one step", () => {
    const value: ChecklistView = {
      ...checklist(),
      steps: [{ number: 1, name: "Vision", mark: "done", count: null, label: "Vision" }],
      items: [{ step: 1, text: "Vision decision missing", path: null, offer: null }],
    };
    renderHome(<FeatureCard checklist={value} />);

    expect(screen.getByText("1 of 1 step done, 0 with gaps")).toBeTruthy();
  });

  it("uses plural labels for zero counts", () => {
    renderHome(<FeatureCard checklist={{ ...checklist(), steps: [], items: [] }} />);

    expect(screen.getByText("0 of 0 steps done, 0 with gaps")).toBeTruthy();
  });

  it("names one folder: the product directory, else the plans directory", () => {
    const { rerender } = renderHome(<FeatureCard checklist={checklist("demo")} />);
    expect(screen.getByText("product/demo/")).toBeTruthy();

    const withPlans = {
      ...checklist("demo"),
      plans: { listed: 2, total: 2, written: 2, running: 0, done: 0 },
    };
    rerender(wrap(<FeatureCard checklist={withPlans} />));
    expect(screen.getByText("product/demo/")).toBeTruthy();
    expect(screen.queryByText(/plans\/demo/)).toBeNull();

    rerender(wrap(<FeatureCard checklist={{ ...withPlans, has_product: false }} />));
    expect(screen.getByText("plans/demo/")).toBeTruthy();
    expect(screen.queryByText(/product\/demo/)).toBeNull();
  });

  it("states sections, plans, untested and running as facts with the right conditions", () => {
    const value: ChecklistView = {
      ...checklist(),
      sections: [
        { number: "01", name: "One", written: true, open_questions: 0 },
        { number: "02", name: "Two", written: false, open_questions: 0 },
      ],
      plans: { listed: 3, total: 3, written: 3, running: 1, done: 0 },
      untested: 2,
    };
    const { container } = renderHome(<FeatureCard checklist={value} />);

    const facts = container.querySelector(".feature-card__facts")?.textContent;
    expect(facts).toBe("1 running2 sections3 plans2 untested");
    expect(container.querySelector(".feature-card__facts [data-kind='running']")).not.toBeNull();
  });

  it("omits running and untested facts at zero and says all done when every step is", () => {
    const steps = checklist().steps.map((step) => ({ ...step, mark: "done" as const }));
    const { container } = renderHome(<FeatureCard checklist={{ ...checklist(), steps }} />);

    expect(container.querySelector(".feature-card__facts")?.textContent).toBe(
      "0 sections0 plansall done",
    );
  });

  it("tints the mark by a stable hash of the slug over --doc-1..8", () => {
    const colour = (slug: string) => {
      const { container, unmount } = renderHome(<FeatureCard checklist={checklist(slug)} />);
      const value = (
        container.querySelector(".feature-mark") as HTMLElement
      ).style.getPropertyValue("--mark");
      unmount();
      return value;
    };

    expect(colour("planner-ui")).toBe(colour("planner-ui"));
    expect(colour("planner-ui")).toMatch(/^var\(--doc-[1-8]\)$/);
    const seen = new Set(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"].map(colour));
    expect(seen.size).toBeGreaterThan(1);
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
    expect(screen.getByText("2 of 6 steps done, 1 with gaps").closest(".feature-pitch")).toBeNull();
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

  it("escapes control characters in slugs and vision pitches", () => {
    renderHome(
      <FeatureCard checklist={{ ...checklist("demo\u202E"), pitch: "Build\u2066 with care." }} />,
    );

    expect(screen.getByRole("heading", { name: "demo⟨U+202E⟩" })).toBeTruthy();
    expect(screen.getByText("product/demo⟨U+202E⟩/")).toBeTruthy();
    expect(screen.getByText("Build⟨U+2066⟩ with care.")).toBeTruthy();
  });
});

describe("FeatureCards", () => {
  it("renders a card for every feature from project state", () => {
    renderHome(<FeatureCards />, {
      features: [checklist("alpha"), checklist("beta")],
    });

    const list = screen.getByRole("list", { name: "Features" });
    expect(within(list).getAllByRole("link")).toHaveLength(2);
    expect(within(list).getByRole("link", { name: "alpha" }).getAttribute("href")).toBe("/f/alpha");
    expect(within(list).getByRole("link", { name: "beta" }).getAttribute("href")).toBe("/f/beta");
  });

  it("renders the lead node as the first list item, and no empty item without one", () => {
    renderHome(<FeatureCards lead={<p>Lead card</p>} />, {
      features: [checklist("alpha")],
    });

    const items = [...screen.getByRole("list", { name: "Features" }).children];
    expect(items).toHaveLength(2);
    expect(within(items[0] as HTMLElement).getByText("Lead card")).toBeTruthy();
    expect(within(items[1] as HTMLElement).getByRole("link", { name: "alpha" })).toBeTruthy();
  });

  it("adds no extra list item without a lead", () => {
    renderHome(<FeatureCards />, { features: [checklist("alpha")] });

    expect(screen.getByRole("list", { name: "Features" }).children).toHaveLength(1);
  });

  it("renders nothing when there are no features", () => {
    const { container } = renderHome(<FeatureCards />);

    expect(container.innerHTML).toBe("");
  });
});
