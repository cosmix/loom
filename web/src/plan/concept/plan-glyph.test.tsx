import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import { PlanGlyph } from "./plan-glyph";

afterEach(cleanup);

it("links a plan slug within the current page's feature", async () => {
  const plans = [{ slug: "feat-x.v2", title: "Feature X", state: "running" }];
  render(
    <MemoryRouter initialEntries={["/f/demo/product/section_01"]}>
      <PlanGlyph plans={plans} sections={[]} />
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Plans implementing this section" }));
  const link = await screen.findByRole("link", { name: "Feature X" });
  expect(link.getAttribute("href")).toBe("/f/demo/engineering/feat-x.v2");
});

it("links a section using its server page key", async () => {
  const sections = [
    {
      number: 1,
      name: "section_01",
      doc: {
        id: "section_01",
        title: "Planning",
        path: "doc/product/demo/section_01.md",
        page: "demo/product/section_01",
      },
    },
  ];
  render(
    <MemoryRouter initialEntries={["/f/other/engineering/feature-x"]}>
      <PlanGlyph plans={[]} sections={sections} />
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Sections this plan implements" }));

  const link = await screen.findByRole("link", { name: "Planning" });
  expect(link.getAttribute("href")).toBe("/f/demo/product/section_01");
});

it("links a plan without a feature to its standalone summary", async () => {
  render(
    <MemoryRouter initialEntries={["/plans/standalone"]}>
      <PlanGlyph plans={[{ slug: "feat-x", title: "Feature X", state: "written" }]} sections={[]} />
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Plans implementing this section" }));

  const link = await screen.findByRole("link", { name: "Feature X" });
  expect(link.getAttribute("href")).toBe("/plans/feat-x");
});
