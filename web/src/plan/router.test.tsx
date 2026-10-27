import { cleanup, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { routes } from "@/plan/router";

vi.mock("@/plan/checklist-view", () => ({
  ChecklistView: () => <div>Checklist page</div>,
}));

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
  it("renders the checklist inside the shell at the index", () => {
    const shell = renderRoute("/");
    expect(shell).not.toBeNull();
    expect(within(shell!).getByText("Checklist page")).toBeTruthy();
  });

  it("renders a registered tab route inside the shell", () => {
    const shell = renderRoute("/files/doc/example.md");
    expect(shell).not.toBeNull();
    expect(within(shell!).getByText("Files page")).toBeTruthy();
    expect(screen.queryByText("Checklist page")).toBeNull();
  });
});
