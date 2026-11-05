import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { authoringViewSchema, serverInfoSchema, type ChecklistView } from "@/plan/api/schema";
import { atlas, offer, planRow } from "@/plan/feature/pages-kit";
import { registerOfferAction } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript (as in pages.contracts.test.tsx).
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const SPINE = "Feature documents";
const ONE = "one";
const PRESSURE_LABEL = `Pressure-test ${ONE}`;
const START_LABEL = "Start a run";
let unregister: (() => void)[] = [];

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

/** The file tree and an idle run state; every other request is a 404. */
function stubServer(): void {
  const idle = {
    ...runStateFixture,
    workspace: "done",
    active_plan: null,
    offer: "start",
    preselected: null,
    abandon: null,
  };
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const { pathname } = new URL(raw, "http://localhost");
    if (pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    if (pathname === "/api/run/state") return Promise.resolve(json(idle));
    return Promise.resolve(
      new Response(JSON.stringify({ error: "not_found", message: "no such page" }), {
        status: 404,
      }),
    );
  });
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

beforeEach(() => {
  window.localStorage.clear();
  stubServer();
  unregister = [registerOfferAction("pressure_test", () => {})];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

/** A read-only server: the fixture's own `read_only` list stays, so every offer is blocked. */
function renderRoute(path: string, checklist: ChecklistView): void {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

const buttonsNamed = (root: HTMLElement, label: string) =>
  within(root)
    .queryAllByRole("button")
    .filter((button) => button.textContent?.includes(label));

it("the spine draws the pressure test offer once on a read-only server when no page owns it", async () => {
  const row = planRow(ONE, "written");
  const test = offer("pressure_test", PRESSURE_LABEL, row.path);
  renderRoute(
    "/f/atlas/vision",
    atlas({
      plan_rows: [row],
      plans: { listed: 1, total: 1, written: 1, running: 0, done: 0 },
      current: { kind: "step", number: 5 },
      primary: test,
      items: [{ step: 5, text: PRESSURE_LABEL, path: row.path, offer: test }],
    }),
  );
  const spine = await screen.findByRole("navigation", { name: SPINE });

  await waitFor(() => expect(buttonsNamed(document.body, PRESSURE_LABEL)).toHaveLength(1));
  expect(buttonsNamed(spine, PRESSURE_LABEL)).toHaveLength(1);
  expect(buttonsNamed(document.body, PRESSURE_LABEL)[0]!.getAttribute("aria-disabled")).toBe(
    "true",
  );
});

it("the spine and the Run page draw the start_run offer once on a read-only server", async () => {
  const row = planRow(ONE, "written");
  renderRoute(
    "/f/atlas/run",
    atlas({
      plan_rows: [row],
      plans: { listed: 1, total: 1, written: 1, running: 0, done: 0 },
      current: { kind: "ready" },
      primary: offer("start_run", START_LABEL, row.path),
    }),
  );
  const spine = await screen.findByRole("navigation", { name: SPINE });

  await waitFor(() => expect(buttonsNamed(document.body, START_LABEL)).toHaveLength(1));
  expect(buttonsNamed(spine, START_LABEL)).toEqual([]);
  const button = buttonsNamed(document.body, START_LABEL)[0]!;
  expect(button.getAttribute("aria-disabled")).toBe("true");
});
