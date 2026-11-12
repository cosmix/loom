import { cleanup, render, screen, waitFor } from "@testing-library/react";
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

/**
 * The bulk settle action is mounted by two routes: the metaplan page (role plan) and the product
 * overview (role section). Both render through the real router; the trace has one section and one
 * plan that need settling.
 */

vi.mock("@/plan/home/home", () => ({ HomeRoute: () => <div>Home page</div> }));
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

function renderRoutes(path: string) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(youAtom, hello.you);
  store.set(authoringAtom, authoringViewSchema.parse({ holder: null }));
  const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]!);
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  render(
    <Provider store={store}>
      <RouterProvider router={createMemoryRouter(routes, { initialEntries: [path] })} />
    </Provider>,
  );
}

it("the metaplan page offers Settle all plans, ahead of the Run panel", async () => {
  renderRoutes("/f/demo/metaplan");

  const settle = await screen.findByRole("button", { name: "Settle all plans" });
  const run = await screen.findByRole("region", { name: "Run" });
  expect(screen.getByText("1 plan needs settling")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Settle all specs" })).toBeNull();
  expect(settle.compareDocumentPosition(run) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("the product overview offers Settle all specs", async () => {
  renderRoutes("/f/demo/product");

  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Settle all specs" })).toBeTruthy(),
  );
  expect(screen.getByText("1 spec needs settling")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Settle all plans" })).toBeNull();
});
