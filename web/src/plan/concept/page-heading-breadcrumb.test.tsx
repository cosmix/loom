import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { projectAtom } from "@/plan/state/atoms";

import { PageHeading } from "./page-heading";
import { pageWith, show } from "./page-heading-kit";

vi.mock("./open-all", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./open-all")>()),
  useOpenAll: () => vi.fn(),
}));
vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: ({ page }: { page: ConceptPage }) => (
    <div role="group" aria-label="Header extras">
      {page.key}
    </div>
  ),
  SummaryAction: () => <span>Summary action</span>,
  SummaryDetail: () => <span>Summary detail</span>,
}));

afterEach(() => {
  cleanup();
});

it.each([
  ["demo/product/section_01", "Product spec", "/f/demo/product", "Section 01"],
  ["overview/demo/product", "Product spec", "/f/demo/product", "Planning"],
  ["demo/engineering/my-plan", "Plans", "/f/demo/metaplan", "my-plan"],
] as const)("renders the area breadcrumb for %s", (key, label, to, current) => {
  show({ key });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).queryByText("Planner")).toBeNull();
  expect(within(nav).getByRole("link", { name: label }).getAttribute("href")).toBe(to);
  expect(nav.querySelector('[aria-current="page"]')?.textContent).toBe(current);
});

it("starts the breadcrumb at the feature's slug, linking to its screen", () => {
  const store = createStore();
  store.set(projectAtom, {
    checklists: [
      {
        ...checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0])),
        feature: "demo",
        sections: Array.from({ length: 12 }, (_, index) => ({
          number: String(index + 1).padStart(2, "0"),
          name: `S${index + 1}`,
          written: true,
          open_questions: 0,
        })),
      },
    ],
    standalone: [],
    moves: [],
  });
  render(<PageHeading page={pageWith({ key: "demo/product/section_05" })} />, {
    wrapper: ({ children }) => (
      <Provider store={store}>
        <MemoryRouter>{children}</MemoryRouter>
      </Provider>
    ),
  });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(
    within(nav)
      .getAllByRole("listitem")
      .map((item) => item.textContent),
  ).toEqual(["demo", "Product spec", "Section 05 of 12"]);
  const first = within(nav).getByRole("link", { name: "demo" });
  expect(first.getAttribute("href")).toBe("/f/demo");
  expect(first.querySelector("svg.lucide-layers")).not.toBeNull();
});

it("sets a plan summary's slug crumb in mono", () => {
  show({ key: "demo/engineering/my-plan" });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByText("my-plan").closest("[aria-current]")?.className).toContain(
    "font-mono",
  );
});

it("heads the vision's summary with the feature and the vision, and opens its source in the feature", () => {
  show({
    key: "demo/product/vision",
    title: "Vision",
    path: "doc/product/demo/index.md",
  });

  const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(
    within(crumbs)
      .getAllByRole("listitem")
      .map((item) => item.textContent),
  ).toEqual(["demo", "Vision"]);
  const source = screen.getByRole("link", { name: "Source" });
  expect(source.getAttribute("href")).toBe("/f/demo/source/product/demo/index.md");
});
