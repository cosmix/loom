import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import hello from "@/plan/api/fixtures/hello.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { routes } from "@/plan/router";
import { authoringAtom, projectAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

/** Contract: the metaplan page is the Plans home; the old engineering address lands on it. */

vi.mock("@/plan/home/home", () => ({ HomeRoute: () => <div>Home page</div> }));
// The layout stands in as a marker that still routes its children.
vi.mock("@/plan/feature/feature-layout", async () => {
  const { createElement } = await import("react");
  const { Outlet } = await import("react-router");
  return {
    FeatureLayout: () => createElement("div", null, "Feature page", createElement(Outlet)),
    FeatureLanding: () => createElement("div", null, "Feature landing"),
  };
});
// A summary page draws what its route puts `above` it; the summary itself is not under test.
vi.mock("@/plan/concept/concept-route", () => ({
  ConceptRoute: ({ above }: { above?: ReactNode }) => (
    <div>
      Concept page
      {above}
    </div>
  ),
}));
vi.mock("@/plan/files/api", () => ({ useFileSocket: () => {} }));

const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });

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
  // The run state and the trace answer; any other request stays out.
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes("/api/trace/")) return Promise.resolve(json(traceFixture));
    if (url.includes("/api/run/state")) {
      return Promise.resolve(json({ ...runStateFixture, workspace: "done", active_plan: null }));
    }
    return new Promise<Response>(() => {});
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** The planner routes at `path`, for the fixture feature `demo`: a metaplan and two written plans. */
function renderRoutes(path: string) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(youAtom, hello.you);
  store.set(authoringAtom, authoringViewSchema.parse({ holder: null }));
  const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]!);
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return router;
}

it("the old engineering address lands on the metaplan page, which carries the Run panel", async () => {
  expect(checklistFixture.checklists[0]!.metaplans).not.toHaveLength(0);
  const router = renderRoutes("/f/demo/engineering");

  await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/metaplan"));
  const panel = await screen.findByRole("region", { name: "Run" });
  expect(within(panel).getAllByRole("button", { name: "Start a run" })).toHaveLength(1);
  expect(screen.queryByText("No such page")).toBeNull();
});
