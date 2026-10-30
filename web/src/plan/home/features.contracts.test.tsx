import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import * as featureCard from "@/plan/home/feature-card";
import {
  checklist,
  elementsWithText,
  project,
  renderFeatureRoute,
  renderWithProject,
  standalonePlans,
} from "@/plan/home/features-contracts-kit";
import * as shelf from "@/plan/home/standalone-shelf";

afterEach(cleanup);

it("features repair feature card shows the title instead of the slug", () => {
  const card = checklist("planner-ui", { title: "Planner UI", sections: 12, plans: 11 });
  const { container } = renderWithProject(
    <featureCard.FeatureCards />,
    project({ checklists: [card] }),
  );

  const list = screen.getByRole("list", { name: "Features" });
  expect(within(list).getByRole("heading", { name: "Planner UI" })).toBeTruthy();
  expect(within(list).queryByRole("heading", { name: "planner-ui" })).toBeNull();
  expect(elementsWithText(container, "12 sections").length).toBeGreaterThan(0);
});

it("features repair standalone shelf starts collapsed with its count", () => {
  renderWithProject(<shelf.StandaloneShelf />, project({ standalone: standalonePlans(30) }));

  const region = screen.getByRole("region", { name: "Standalone plans" });
  const details = region.querySelector("details");
  expect(details).not.toBeNull();
  expect(details?.open).toBe(false);
  const summary = details?.querySelector(":scope > summary");
  expect(summary?.textContent?.replace(/\s+/g, " ")).toContain("30 standalone plans");
  const planLinks = within(region)
    .queryAllByRole("link")
    .filter((link) => link.getAttribute("href")?.startsWith("/files/plans/"));
  for (const link of planLinks) {
    expect(details?.contains(link)).toBe(true);
    expect(summary?.contains(link)).toBe(false);
  }
});

it("features repair feature screen selects the step from the query", () => {
  const demo = checklist("demo", { title: "Demo", sections: 3, plans: 2, current: 2 });
  renderFeatureRoute("/f/demo?step=4", project({ checklists: [demo] }));

  const nav = screen.getByRole("navigation", { name: "Steps" });
  const plans = within(nav)
    .getAllByRole("link")
    .find((link) => /\bPlans\b/.test(link.textContent ?? ""));
  expect(plans).toBeDefined();
  expect(plans?.getAttribute("aria-current")).toBe("step");
  expect(screen.getByRole("heading", { level: 1, name: "Write the plans" })).toBeTruthy();
  expect(screen.queryByRole("heading", { level: 1, name: "Write the product spec" })).toBeNull();
});
