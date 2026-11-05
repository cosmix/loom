import { cleanup, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { routes } from "@/plan/router";
import { projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/home/home", () => ({ HomeRoute: () => <div>Home page</div> }));
// The layout stands in as a marker that still routes its children, so the catch-all is reachable.
vi.mock("@/plan/feature/feature-layout", async () => {
  const { createElement } = await import("react");
  const { Outlet } = await import("react-router");
  return {
    FeatureLayout: () => createElement("div", null, "Feature page", createElement(Outlet)),
    FeatureLanding: () => createElement("div", null, "Feature landing"),
  };
});
vi.mock("@/plan/concept/concept-route", () => ({ ConceptRoute: () => <div>Concept page</div> }));
vi.mock("@/plan/files/api", () => ({ useFileSocket: () => {} }));
vi.mock("@/plan/files/reading-column", () => ({
  ReadingColumn: ({ path, anchor }: { path: string; anchor: string | null }) => (
    <div>{`Source of ${path} at ${anchor}`}</div>
  ),
}));
vi.mock("@/plan/feature/run-page", () => ({ RunPage: () => <div>Run page</div> }));
vi.mock("@/plan/new/kickoff-route", () => ({ KickoffRoute: () => <div>Kickoff page</div> }));

// vi.mock factories are hoisted above the JSX runtime import, so the element is built from an
// awaited React import instead of JSX.
vi.mock("@/plan/tabs", async () => {
  const { createElement } = await import("react");
  return {
    PLAN_TABS: [],
    PLAN_TAB_ROUTES: [{ path: "files/*", element: createElement("div", null, "Files page") }],
  };
});

beforeEach(() => {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) =>
      ({
        matches: true,
        media: query,
        addEventListener() {},
        removeEventListener() {},
      }) as unknown as MediaQueryList,
  );
});

afterEach(() => {
  getDefaultStore().set(projectAtom, null);
  cleanup();
  vi.restoreAllMocks();
});

function seedFeature(patch: Record<string, unknown>) {
  const checklist = checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "demo",
    ...patch,
  });
  getDefaultStore().set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
}

function renderRoute(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const view = render(<RouterProvider router={router} />);
  return view.container.querySelector<HTMLElement>("[data-loom-plan]");
}

describe("plan routes", () => {
  it("renders home inside the shell at the index", () => {
    const shell = renderRoute("/");
    expect(shell).not.toBeNull();
    expect(within(shell!).getByText("Home page")).toBeTruthy();
  });

  it("renders the feature layout around its landing at /f/:feature", () => {
    const shell = renderRoute("/f/demo");
    expect(within(shell!).getByText("Feature page")).toBeTruthy();
    expect(within(shell!).getByText("Feature landing")).toBeTruthy();
  });

  it("routes /f/demo/run to its page inside the feature layout", () => {
    const shell = renderRoute("/f/demo/run");
    expect(within(shell!).getByText("Feature page")).toBeTruthy();
    expect(within(shell!).getByText("Run page")).toBeTruthy();
    expect(within(shell!).queryByText("No such page")).toBeNull();
  });

  it("has no Pressure page: the route answers like any mistyped feature page", () => {
    const shell = renderRoute("/f/demo/pressure");
    expect(within(shell!).getByText("No such page")).toBeTruthy();
  });

  it("answers a feature page that has no route with an error, not a blank page", () => {
    const shell = renderRoute("/f/demo/nowhere");
    expect(within(shell!).getByText("Feature page")).toBeTruthy();
    expect(within(shell!).getByText("No such page")).toBeTruthy();
  });

  it("renders the kickoff conversation at /new", () => {
    const shell = renderRoute("/new");
    expect(within(shell!).getByText("Kickoff page")).toBeTruthy();
  });

  it("renders the Changed since you looked page at /changes, not a dialog", () => {
    const shell = renderRoute("/changes");
    expect(
      within(shell!).getByRole("heading", { level: 1, name: "Changed since you looked" }),
    ).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders a registered tab route inside the shell", () => {
    const shell = renderRoute("/files/doc/example.md");
    expect(shell).not.toBeNull();
    expect(within(shell!).getByText("Files page")).toBeTruthy();
    expect(screen.queryByText("Home page")).toBeNull();
  });

  it("keeps the offer to start a vision on /vision for a feature without one", () => {
    seedFeature({ has_product: false });
    const shell = renderRoute("/f/demo/vision");
    expect(within(shell!).getByText("This feature has no vision yet.")).toBeTruthy();
    expect(within(shell!).queryByText("Concept page")).toBeNull();
  });

  it("shows the vision's summary page on /vision for a feature with a vision", () => {
    seedFeature({ has_product: true });
    const shell = renderRoute("/f/demo/vision");
    expect(within(shell!).getByText("Concept page")).toBeTruthy();
    expect(within(shell!).queryByText("This feature has no vision yet.")).toBeNull();
  });

  it("links the vision's summary page to the whole vision", () => {
    seedFeature({ has_product: true });
    const shell = renderRoute("/f/demo/vision");
    const link = within(shell!).getByRole("link", { name: "Read the full vision" });
    expect(link.getAttribute("href")).toBe("/f/demo/source/product/demo/index.md");
    expect(link.classList.contains("concept-read-source")).toBe(true);
  });

  it("offers no link to the vision's source while the vision is missing", () => {
    seedFeature({ has_product: false });
    const shell = renderRoute("/f/demo/vision");
    expect(within(shell!).queryByRole("link", { name: "Read the full vision" })).toBeNull();
  });

  it.each(["/f/demo/metaplan", "/f/demo/metaplan/index.md"])(
    "shows the metaplan's summary page at %s, not its source",
    (path) => {
      seedFeature({ metaplans: [{ path: "doc/plans/demo/index.md", title: "Roadmap" }] });
      const shell = renderRoute(path);
      expect(within(shell!).getByText("Concept page")).toBeTruthy();
      expect(within(shell!).queryByText(/^Source of/)).toBeNull();
    },
  );

  it("moves a first metaplan that is not the index to its own file route", () => {
    seedFeature({ metaplans: [{ path: "doc/plans/demo/round-two.md", title: "Round two" }] });
    const router = createMemoryRouter(routes, { initialEntries: ["/f/demo/metaplan"] });
    render(<RouterProvider router={router} />);
    expect(router.state.location.pathname).toBe("/f/demo/metaplan/round-two.md");
    expect(screen.getByText("Concept page")).toBeTruthy();
  });

  it("keeps the offer to start a metaplan while there is none, and refuses an unknown file", () => {
    seedFeature({ metaplans: [] });
    const none = renderRoute("/f/demo/metaplan");
    expect(within(none!).getByText("This feature has no metaplan yet.")).toBeTruthy();
    cleanup();
    seedFeature({ metaplans: [{ path: "doc/plans/demo/index.md", title: "Roadmap" }] });
    const shell = renderRoute("/f/demo/metaplan/other.md");
    expect(within(shell!).getByText("No such metaplan")).toBeTruthy();
    expect(within(shell!).queryByText("Concept page")).toBeNull();
  });

  it.each([
    ["/f/demo/source/product/demo/index.md", "doc/product/demo/index.md"],
    ["/f/demo/source/plans/demo/PLAN-x.md#stages", "doc/plans/demo/PLAN-x.md"],
  ])("opens the source of %s inside the feature layout", (path, repoPath) => {
    seedFeature({ has_product: true });
    const shell = renderRoute(path);
    expect(within(shell!).getByText("Feature page")).toBeTruthy();
    expect(within(shell!).getByText(new RegExp(`^Source of ${repoPath} at`))).toBeTruthy();
  });

  it("scrolls the source to the link's anchor", () => {
    seedFeature({ has_product: true });
    const shell = renderRoute("/f/demo/source/product/demo/section_01.md#the%20intent");
    const shown = "Source of doc/product/demo/section_01.md at the intent";
    expect(within(shell!).getByText(shown)).toBeTruthy();
  });

  it.each([
    "/f/demo/source/product/other/index.md",
    "/f/demo/source/plans/PLAN-x.md",
    "/f/demo/source/knowledge/demo/x.md",
    "/f/demo/source/product/demo/../other/index.md",
    "/f/demo/source",
  ])("refuses %s, which is no document of the feature", (path) => {
    seedFeature({ has_product: true });
    const shell = renderRoute(path);
    expect(within(shell!).getByText("Not a document of this feature")).toBeTruthy();
    expect(within(shell!).queryByText(/^Source of/)).toBeNull();
  });
});
