import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  type OfferView,
} from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript. This stubs out its registrations
// too, so the offer these contracts need is registered below.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const WIDE_QUERY = "(min-width: 1024px)";
const METAPLAN_KEY = "atlas/metaplan/index";
const CANDIDATES = "Candidate follow-ups";
const FINDING = "Settings statistics look bland";
const MARKUP = '<img src=x onerror="alert(1)"> Bold **claim**';

let calls: { offer: OfferView; seed: unknown }[] = [];
let unregister: (() => void)[] = [];

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

function atlas(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    ...patch,
  });
}

function donePlan(slug: string) {
  return {
    slug,
    path: `doc/plans/atlas/archive/DONE-PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state: "done",
    pressure_date: null,
    listed: true,
  };
}

function built(candidateText: string) {
  return {
    reviews: [
      { slug: "atlas-01", path: "doc/plans/REVIEW-PLAN-atlas-01.md" },
      { slug: "atlas-02", path: "doc/plans/atlas/archive/REVIEW-PLAN-atlas-02.md" },
    ],
    spec: [
      {
        stage: "Integration Verification",
        stage_id: "integration-verify",
        text: "Corrected section_01 to match the build",
        review: "doc/plans/REVIEW-PLAN-atlas-01.md",
      },
    ],
    candidates: [
      {
        id: "F-1",
        kind: "finding",
        stage: "Chat polish",
        stage_id: "chat-polish",
        text: candidateText,
        review: "doc/plans/REVIEW-PLAN-atlas-01.md",
      },
    ],
  };
}

/** Every plan done: no primary, Plan follow-ups as the secondary, one open finding in a review. */
function finishedChecklist(candidateText = FINDING): ChecklistView {
  return atlas({
    plan_rows: [donePlan("atlas-01"), donePlan("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [
      { id: "plan-follow-ups", kind: "plan_follow_ups", label: "Plan follow-ups", target: null },
    ],
    items: [],
    built: built(candidateText),
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
  calls = [];
  stubMedia(true);
  stubServer();
  unregister = [
    registerOfferAction("plan_follow_ups", (offer, seed) => {
      calls.push({ offer, seed });
    }),
  ];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

it("an all-done feature lands on its Built state", async () => {
  const { router } = renderRoutes("/f/atlas", finishedChecklist());

  await waitFor(() => expect(router.state.location.pathname).toBe("/f/atlas/built"));
  expect(await screen.findByRole("heading", { level: 1, name: "Built" })).toBeTruthy();
});

it("Plan this starts Plan follow-ups seeded with the finding and its source", async () => {
  renderRoutes("/f/atlas/built", finishedChecklist());

  const list = await screen.findByRole("list", { name: CANDIDATES });
  const item = within(list)
    .getAllByRole("listitem")
    .find((row) => row.textContent?.includes(FINDING));
  if (!item) throw new Error(`no candidate names "${FINDING}"`);
  fireEvent.click(within(item).getByRole("button", { name: "Plan this" }));

  expect(calls).toHaveLength(1);
  expect(calls[0]!.offer.kind).toBe("plan_follow_ups");
  const seed = String(calls[0]!.seed);
  expect(seed).toContain(FINDING);
  expect(seed).toContain("REVIEW-PLAN-atlas-01.md");
  expect(seed).toContain("chat-polish");
});

it("a finding with markup renders as plain text on the Built page", async () => {
  renderRoutes("/f/atlas/built", finishedChecklist(MARKUP));

  const list = await screen.findByRole("list", { name: CANDIDATES });
  expect(list.querySelectorAll("img")).toHaveLength(0);
  expect(list.querySelectorAll("strong")).toHaveLength(0);
  expect(list.textContent).toContain("<img src=x");
  expect(list.textContent).toContain("**claim**");
});
