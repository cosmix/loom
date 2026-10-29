import { cleanup, render } from "@testing-library/react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ReadingFoldScene } from "./reading-scene";
import { SceneRouter } from "./scene-router";
import { PLANNER_HELP_TOPICS } from "./topics";

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Help must not request live data")));
  vi.stubGlobal("WebSocket", vi.fn());
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it.each(PLANNER_HELP_TOPICS.flatMap(({ scenes }) => scenes))(
  "renders $title inside the app router without replacing its location",
  ({ Scene }) => {
    const router = createMemoryRouter([{ path: "*", element: <Scene /> }], {
      initialEntries: ["/f/atlas/product/map-view?help=open#topic"],
    });

    const { container } = render(<RouterProvider router={router} />);

    // RouterProvider catches render errors, so a successful render call alone misses the crash.
    expect(router.state.errors).toBeNull();
    expect(container.querySelector("[data-loom-plan][inert]")).not.toBeNull();
    expect(router.state.location).toMatchObject({
      pathname: "/f/atlas/product/map-view",
      search: "?help=open",
      hash: "#topic",
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  },
);

it("renders the reading fold scene outside any router", () => {
  const { container } = render(<ReadingFoldScene />);

  expect(container.querySelector("[data-loom-plan][inert] .concept-passage-chip")).not.toBeNull();
  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});

function LocationProbe() {
  const { pathname } = useLocation();
  return <output>{pathname}</output>;
}

it("uses the standalone scene's initial route", () => {
  const { container } = render(
    <SceneRouter initialEntries={["/files"]}>
      <LocationProbe />
    </SceneRouter>,
  );

  expect(container.querySelector("output")?.textContent).toBe("/files");
});

it("lets scene children read the app location when a router already exists", () => {
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: (
          <SceneRouter initialEntries={["/files"]}>
            <LocationProbe />
          </SceneRouter>
        ),
      },
    ],
    { initialEntries: ["/new"] },
  );

  const { container } = render(<RouterProvider router={router} />);

  expect(container.querySelector("output")?.textContent).toBe("/new");
});
