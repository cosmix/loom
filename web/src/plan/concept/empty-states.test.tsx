import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { authoringViewSchema, checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema, type EmptyState } from "@/plan/api/schema-concept";
import { docViewSchema } from "@/plan/api/schema-files";
import { fetchDoc } from "@/plan/files/api";
import { registerOfferAction } from "@/plan/offers/actions";
import {
  activeFeatureAtom,
  authoringAtom,
  checklistAtom,
  projectAtom,
  serverInfoAtom,
} from "@/plan/state/atoms";

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

const METAPLAN_OFFER = {
  id: "start-metaplan",
  kind: "start_metaplan",
  label: "Start the metaplan",
  target: null,
} as const;

const EMPTY_METAPLAN: EmptyState = {
  kind: "no_metaplan",
  count: null,
  offer: "start-metaplan",
  docs: [],
};

/** A writable, unheld store: otherwise EmptyOffer draws text, and no button proves nothing. */
function writableStore() {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  return store;
}

/** The empty `no_metaplan` overview, drawn at `path` (a route with `:feature` or none at all). */
function renderEmptyMetaplan(store: ReturnType<typeof createStore>, path: string) {
  const page = conceptPageSchema.parse({ ...overviewFixture, empty: EMPTY_METAPLAN });
  const element = <EmptyStates page={page} empty={EMPTY_METAPLAN} />;
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/f/:feature/*" element={element} />
          <Route path="*" element={element} />
        </Routes>
      </MemoryRouter>
    </Provider>,
  );
}

/** An empty overview offering `start-metaplan` on a checklist whose primary is `primary`. */
function showMetaplanOffer(
  primary: typeof METAPLAN_OFFER | null,
  secondary = [METAPLAN_OFFER],
  path = "/f/demo/product",
) {
  const store = writableStore();
  store.set(
    checklistAtom,
    checklistViewSchema.parse({ ...checklistFixture.checklists[0], primary, secondary }),
  );
  renderEmptyMetaplan(store, path);
}

/** Two features: `demo` is active, `other` is the route's. Each lists the offer as `primary`. */
function showTwoFeatures(primaries: { demo: boolean; other: boolean }, routeFeature: string) {
  const store = writableStore();
  const base = checklistFixture.checklists[0];
  const view = (feature: string, owns: boolean) =>
    checklistViewSchema.parse({
      ...base,
      feature,
      primary: owns ? METAPLAN_OFFER : null,
      secondary: owns ? [] : [METAPLAN_OFFER],
    });
  store.set(projectAtom, {
    checklists: [view("demo", primaries.demo), view("other", primaries.other)],
    standalone: [],
    moves: [],
  });
  store.set(activeFeatureAtom, "demo");
  renderEmptyMetaplan(store, `/f/${routeFeature}/product`);
}

it("leaves the primary offer to the spine and keeps its sentence", () => {
  const unregister = registerOfferAction("start_metaplan", vi.fn());
  showMetaplanOffer(METAPLAN_OFFER, []);

  expect(screen.getByText("No metaplan yet")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Start the metaplan" })).toBeNull();
  unregister();
});

it("draws an offer that is not the primary", () => {
  const unregister = registerOfferAction("start_metaplan", vi.fn());
  showMetaplanOffer(null);

  expect(screen.getByRole("button", { name: "Start the metaplan" })).toBeTruthy();
  unregister();
});

it("draws the offer on a standalone page, which has no route feature and no spine", () => {
  const unregister = registerOfferAction("start_metaplan", vi.fn());
  showMetaplanOffer(METAPLAN_OFFER, [], "/plans/demo");

  expect(screen.getByRole("button", { name: "Start the metaplan" })).toBeTruthy();
  unregister();
});

it("suppresses the offer by the route feature's primary, not the active feature's", () => {
  const unregister = registerOfferAction("start_metaplan", vi.fn());
  // The route's feature `other` owns the primary; the active `demo` does not. The spine draws it.
  showTwoFeatures({ demo: false, other: true }, "other");
  expect(screen.queryByRole("button", { name: "Start the metaplan" })).toBeNull();
  cleanup();

  // The active `demo` owns the primary; the route's `other` does not. Nothing else draws it.
  showTwoFeatures({ demo: true, other: false }, "other");
  expect(screen.getByRole("button", { name: "Start the metaplan" })).toBeTruthy();
  unregister();
});

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
