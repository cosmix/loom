import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  serverInfoSchema,
  type ChecklistView,
} from "@/plan/api/schema";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const SPINE = "Feature documents";
const WIDE_QUERY = "(min-width: 1024px)";
const METAPLAN_KEY = "atlas/metaplan/index";
const FIRST_SENTENCE = summaryFixture.blocks.flatMap((block) =>
  "sentences" in block ? block.sentences : [],
)[0]!.text;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function requestUrl(input: RequestInfo | URL): URL {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost");
}

function summaryPage(key: string, title: string, path: string): unknown {
  return { ...structuredClone(summaryFixture), key, title, feature: "atlas", path };
}

/** The server: the atlas overview, the vision and metaplan summaries and the file tree, else 404. */
function stubServer(): void {
  const notFound = () => json({ error: "not_found", message: "no such page" }, 404);
  const pages: Record<string, unknown> = {
    "overview/atlas/product": {
      ...overviewFixture,
      key: "overview/atlas/product",
      feature: "atlas",
      path: "doc/product/atlas/index.md",
    },
    "atlas/product/vision": summaryPage(
      "atlas/product/vision",
      "Vision",
      "doc/product/atlas/index.md",
    ),
    [METAPLAN_KEY]: summaryPage(METAPLAN_KEY, "Metaplan", "doc/plans/atlas/index.md"),
  };
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const url = requestUrl(input);
    if (url.pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    const key = decodeURIComponent(url.pathname.replace("/api/concept/", ""));
    return Promise.resolve(key in pages ? json(pages[key]) : notFound());
  });
}

function stubMedia(wide: boolean): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: wide && query === WIDE_QUERY,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

function section(number: string, name: string) {
  return { number, name, written: true, open_questions: 0 };
}

function plan(slug: string, state: string, pressureDate: string | null) {
  const prefix = state === "done" ? "DONE-" : "";
  return {
    slug,
    path: `doc/plans/atlas/${prefix}PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state,
    pressure_date: pressureDate,
    listed: true,
  };
}

function offer(kind: string, label: string) {
  return { id: kind, kind, label, target: null, feature: "atlas" };
}

function atlas(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    ...patch,
  });
}

/** Every listed section written and every plan done: a feature with nothing left to start. */
function finishedChecklist(): ChecklistView {
  return atlas({
    sections: [
      section("01", "Workflow model"),
      section("02", "Command surface"),
      section("03", "Review path"),
    ],
    plan_rows: [plan("atlas-01", "done", null), plan("atlas-02", "done", null)],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [offer("plan_follow_ups", "Plan follow-ups")],
    items: [],
  });
}

/** Two written plans, one pressure tested and one not. */
function pressureChecklist(): ChecklistView {
  return atlas({
    ...finishedChecklist(),
    plan_rows: [plan("atlas-tested", "written", "2026-10-20"), plan("atlas-open", "written", null)],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 0 },
    untested: 1,
  });
}

function renderRoutes(path: string, checklist: ChecklistView) {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { router, store };
}

beforeEach(() => {
  window.localStorage.clear();
  stubMedia(true);
  stubServer();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("a feature with a written spec opens on its product overview", async () => {
  const { router } = renderRoutes("/f/atlas", finishedChecklist());

  await waitFor(() => expect(router.state.location.pathname).toBe("/f/atlas/product"));
});

it("the metaplan route shows the metaplan summary with Source one click away", async () => {
  renderRoutes("/f/atlas/metaplan", finishedChecklist());

  const column = await screen.findByRole("document", { name: "Reading column" });
  await waitFor(() => expect(column.textContent).toContain(FIRST_SENTENCE));

  const sources = await screen.findAllByRole("link", { name: "Source" });
  const hrefs = sources.map((link) => link.getAttribute("href") ?? "");
  expect(
    hrefs.some((href) => href.startsWith("/f/atlas/source/plans/atlas/")),
    `${hrefs}`,
  ).toBe(true);
});

it("plan rows carry the pressure record and the spine has no Pressure test row", async () => {
  renderRoutes("/f/atlas/engineering", pressureChecklist());

  const spine = await screen.findByRole("navigation", { name: SPINE });
  const rows = within(spine);
  const tested = rows.getByRole("link", { name: /\batlas-tested\b/ });
  const open = rows.getByRole("link", { name: /\batlas-open\b/ });
  expect(tested.textContent).toContain("Pressure-tested 2026-10-20");
  expect(open.textContent).toContain("Not pressure-tested");
  expect(rows.queryAllByRole("link", { name: /^Pressure test\b/ })).toEqual([]);
});
