import { cleanup, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { routes } from "@/plan/router";

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
  cleanup();
  vi.restoreAllMocks();
});

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

  it("answers a feature page that has no route with an error, not a blank page", () => {
    const shell = renderRoute("/f/demo/pressure");
    expect(within(shell!).getByText("Feature page")).toBeTruthy();
    expect(within(shell!).getByText("No such page")).toBeTruthy();
  });

  it("renders the kickoff conversation at /new", () => {
    const shell = renderRoute("/new");
    expect(within(shell!).getByText("Kickoff page")).toBeTruthy();
  });

  it("renders a registered tab route inside the shell", () => {
    const shell = renderRoute("/files/doc/example.md");
    expect(shell).not.toBeNull();
    expect(within(shell!).getByText("Files page")).toBeTruthy();
    expect(screen.queryByText("Home page")).toBeNull();
  });
});
