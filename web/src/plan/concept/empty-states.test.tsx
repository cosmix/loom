import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import { conceptPageSchema, type EmptyState } from "@/plan/api/schema-concept";
import { docViewSchema } from "@/plan/api/schema-files";
import { fetchDoc } from "@/plan/files/api";

import { ConceptNotFound, EmptyStates } from "./empty-states";

vi.mock("@/plan/files/api", () => ({ fetchDoc: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const doc = {
  id: "section_01",
  title: "Planning",
  path: "doc/product/demo/section_01.md",
  page: "demo/product/section_01",
};

function showEmpty(kind: EmptyState["kind"]) {
  const empty: EmptyState = { kind, count: 1, offer: null, docs: [doc] };
  const page = conceptPageSchema.parse({ ...overviewFixture, empty });
  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <EmptyStates page={page} empty={empty} />
      </MemoryRouter>
    </Provider>,
  );
}

it("links listed sections to feature-qualified summaries", () => {
  showEmpty("sections_no_vision");

  expect(screen.getByRole("link", { name: "Planning" }).getAttribute("href")).toBe(
    "/f/demo/product/section_01",
  );
});

it("keeps never-built documents linked to file mode", () => {
  showEmpty("never_built");

  expect(screen.getByRole("link", { name: "Planning" }).getAttribute("href")).toBe(
    "/files/product/demo/section_01.md",
  );
});

it.each(["overview/demo/product", "overview/demo/engineering"])(
  "links back to the feature's overviews from missing page %s",
  async (pageKey) => {
    render(
      <MemoryRouter>
        <ConceptNotFound pageKey={pageKey} />
      </MemoryRouter>,
    );

    const product = await screen.findByRole("link", { name: "Product overview" });
    expect(product.getAttribute("href")).toBe("/f/demo/product");
    expect(screen.getByRole("link", { name: "Engineering overview" }).getAttribute("href")).toBe(
      "/f/demo/engineering",
    );
    expect(fetchDoc).not.toHaveBeenCalled();
  },
);

it("links a missing section back to its feature's overviews", async () => {
  vi.mocked(fetchDoc).mockResolvedValue({
    ok: false,
    status: 404,
    error: "not_found",
    message: "not found",
  });
  render(
    <MemoryRouter>
      <ConceptNotFound pageKey="demo/product/section_01" />
    </MemoryRouter>,
  );

  const product = await screen.findByRole("link", { name: "Product overview" });
  expect(product.getAttribute("href")).toBe("/f/demo/product");
  expect(screen.getByRole("link", { name: "Engineering overview" }).getAttribute("href")).toBe(
    "/f/demo/engineering",
  );
  expect(fetchDoc).toHaveBeenCalledWith("doc/product/demo/section_01.md");
});

it("offers file mode when the feature's source section exists without a summary", async () => {
  vi.mocked(fetchDoc).mockResolvedValue({
    ok: true,
    value: docViewSchema.parse({ ...docFixture, path: doc.path }),
  });
  render(
    <MemoryRouter>
      <ConceptNotFound pageKey="demo/product/section_01" />
    </MemoryRouter>,
  );

  const link = await screen.findByRole("link", { name: "Open in file mode" });
  expect(link.getAttribute("href")).toBe("/files/product/demo/section_01.md");
});

it.each([
  {
    pageKey: "demo/engineering/x",
    source: "doc/plans/demo/PLAN-x.md",
    href: "/files/plans/demo/PLAN-x.md",
  },
  {
    pageKey: "plans/x",
    source: "doc/plans/PLAN-x.md",
    href: "/files/plans/PLAN-x.md",
  },
])(
  "offers file mode for an existing plan without a summary at $pageKey",
  async ({ pageKey, source, href }) => {
    vi.mocked(fetchDoc).mockResolvedValue({
      ok: true,
      value: docViewSchema.parse({ ...docFixture, path: source }),
    });
    render(
      <MemoryRouter>
        <ConceptNotFound pageKey={pageKey} />
      </MemoryRouter>,
    );

    const link = await screen.findByRole("link", { name: "Open in file mode" });
    expect(link.getAttribute("href")).toBe(href);
    expect(fetchDoc).toHaveBeenCalledWith(source);
  },
);

it("offers Planner for a missing standalone plan with no feature overview", async () => {
  vi.mocked(fetchDoc).mockResolvedValue({
    ok: false,
    status: 404,
    error: "not_found",
    message: "not found",
  });
  render(
    <MemoryRouter>
      <ConceptNotFound pageKey="plans/feature-x" />
    </MemoryRouter>,
  );

  expect((await screen.findByRole("link", { name: "Planner" })).getAttribute("href")).toBe("/");
  expect(fetchDoc).toHaveBeenCalledWith("doc/plans/PLAN-feature-x.md");
});
