import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  serverInfoSchema,
  type ChecklistView,
  type OfferKind,
  type OfferView,
} from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript. This stubs out its registrations
// too, so the offers these contracts need are registered below.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const SPINE = "Feature documents";
const TESTED = "ledger";
const UNTESTED = "gateway";
const TESTED_DATE = "2026-10-20";
const COMPARE = "Compare spec with build";

let compareRuns: OfferView[] = [];
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

/** The server: the file tree and a run state with nothing in flight, and 404 for the rest. */
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
    const { pathname } = requestUrl(input);
    if (pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    if (pathname === "/api/run/state") return Promise.resolve(json(idle));
    return Promise.resolve(json({ error: "not_found", message: "no such page" }, 404));
  });
}

function stubWideScreen(): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

function offer(kind: OfferKind, label: string, target: string | null = null) {
  return { id: kind, kind, label, target, feature: "atlas" };
}

function planRow(slug: string, state: string, pressure_date: string | null) {
  return {
    slug,
    path: `doc/plans/atlas/PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state,
    pressure_date,
    listed: true,
  };
}

function atlas(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    ...patch,
  });
}

/** Every plan done, so the feature has no primary and the retro is a secondary. */
function finishedChecklist(): ChecklistView {
  return atlas({
    plan_rows: [planRow(TESTED, "done", TESTED_DATE), planRow(UNTESTED, "done", TESTED_DATE)],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [offer("refine_spec", COMPARE), offer("new_metaplan_round", "New metaplan round")],
    items: [],
  });
}

function renderRoute(path: string, checklist: ChecklistView): void {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

/** The routed page beside the spine: the spine lists every plan too, so it proves nothing here. */
async function routedPage(): Promise<HTMLElement> {
  await screen.findByRole("navigation", { name: SPINE });
  const page = document.querySelector<HTMLElement>(".plan-layout__page");
  if (!page) throw new Error("the feature layout drew no page");
  return page;
}

/** The largest element around the plan's name that names no other plan, whatever draws the row. */
async function planRowOf(page: HTMLElement, slug: string, other: string): Promise<HTMLElement> {
  const [name] = await within(page).findAllByText(slug);
  let row = name!;
  while (row.parentElement && row.parentElement !== page) {
    if (row.parentElement.textContent?.includes(other)) break;
    row = row.parentElement;
  }
  return row;
}

function offerButtons(root: HTMLElement, label: string): HTMLElement[] {
  return within(root)
    .queryAllByRole("button")
    .filter((button) => button.textContent?.includes(label));
}

beforeEach(() => {
  window.localStorage.clear();
  compareRuns = [];
  stubWideScreen();
  stubServer();
  unregister = [
    registerOfferAction("pressure_test", () => {}),
    registerOfferAction("refine_spec", (run) => compareRuns.push(run)),
    registerOfferAction("new_metaplan_round", () => {}),
  ];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

it("a finished feature's Run page holds the compare offer once", async () => {
  renderRoute("/f/atlas/run", finishedChecklist());
  const page = await routedPage();

  // A real button that starts the retro: a page naming the offer in prose starts nothing.
  await waitFor(() => expect(offerButtons(page, COMPARE)).toHaveLength(1));
  fireEvent.click(offerButtons(page, COMPARE)[0]!);
  expect(compareRuns.map((run) => run.kind)).toEqual(["refine_spec"]);

  const spine = screen.getByRole("navigation", { name: SPINE });
  expect(offerButtons(spine, COMPARE)).toEqual([]);
  expect(offerButtons(document.body, COMPARE)).toHaveLength(1);

  await planRowOf(page, TESTED, UNTESTED);
  await planRowOf(page, UNTESTED, TESTED);
});
