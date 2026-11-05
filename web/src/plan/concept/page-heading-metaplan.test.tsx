import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { featureOfKey } from "@/plan/api/page-key";
import { conceptPageSchema, type ConceptPage } from "@/plan/api/schema-concept";

import { PageHeading } from "./page-heading";

const openAll = vi.hoisted(() => vi.fn());
vi.mock("./open-all", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./open-all")>()),
  useOpenAll: () => openAll,
}));
vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: ({ page }: { page: ConceptPage }) => (
    <div role="group" aria-label="Header extras">
      {page.key}
    </div>
  ),
  SummaryAction: () => <span>Summary action</span>,
}));

afterEach(() => {
  cleanup();
  openAll.mockClear();
});

function pageWith(overrides: Partial<ConceptPage> = {}): ConceptPage {
  return conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    state: null,
    implemented_by: [],
    feature: featureOfKey(overrides.key ?? summaryFixture.key),
    ...overrides,
  });
}

function show(overrides: Partial<ConceptPage> = {}) {
  const page = pageWith(overrides);
  return render(<PageHeading page={page} />, { wrapper: MemoryRouter });
}

it("heads a metaplan's summary with the feature and Metaplan, and opens its source in the feature", () => {
  show({
    key: "demo/metaplan/round-two",
    title: "Round two",
    path: "doc/plans/demo/round-two.md",
  });

  const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(
    within(crumbs)
      .getAllByRole("listitem")
      .map((item) => item.textContent),
  ).toEqual(["demo", "Metaplan"]);
  const source = screen.getByRole("link", { name: "Source" });
  expect(source.getAttribute("href")).toBe("/f/demo/source/plans/demo/round-two.md");
});
