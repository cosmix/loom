import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, MemoryRouter, RouterProvider } from "react-router";
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
import { PlanHeader } from "@/plan/header/header";
import { registerOfferAction } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript. Removing the offers the panel
// draws belongs to the chat stage, and this stubs out its registrations too, so the offers these
// contracts need are registered below.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

type Store = ReturnType<typeof createStore>;

const SPINE = "Feature documents";
const VISION_SENTENCE = "A plan starts from a short statement of intent.";
const WIDE_QUERY = "(min-width: 1024px)";
const SECTION_NAMES = { "01": "Workflow model", "02": "Command surface", "03": "Review path" };

/** Case-sensitive, so a row may carry a mark or a count beside its name. */
const SPINE_ROWS = [/\bVision\b/, /\bProduct spec\b/, /\bMetaplan\b/, /\bPlans\b/, /\bRun\b/];

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

/** The server: the atlas overview, the atlas vision summary and the file tree, and 404 for the rest. */
function stubServer(): void {
  const notFound = () => json({ error: "not_found", message: "no such page" }, 404);
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const url = requestUrl(input);
    if (url.pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    const key = decodeURIComponent(url.pathname.replace("/api/concept/", ""));
    if (key === "atlas/product/vision") return Promise.resolve(json(visionPage()));
    return Promise.resolve(key === "overview/atlas/product" ? json(overview()) : notFound());
  });
}

function overview(): unknown {
  return {
    ...overviewFixture,
    key: "overview/atlas/product",
    feature: "atlas",
    path: "doc/product/atlas/index.md",
  };
}

function visionPage(): unknown {
  return {
    ...summaryFixture,
    key: "atlas/product/vision",
    title: "Vision",
    feature: "atlas",
    path: "doc/product/atlas/index.md",
  };
}

function stubMedia(wide: boolean): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: wide && query === WIDE_QUERY,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

function section(number: keyof typeof SECTION_NAMES, written: boolean) {
  return { number, name: SECTION_NAMES[number], written, open_questions: 0 };
}

function writeSection(number: 2 | 3) {
  return {
    id: `write-section:0${number}`,
    kind: "write_section",
    label: `Write section 0${number}`,
    target: `doc/product/atlas/section_0${number}.md`,
    feature: "atlas",
    number,
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
  const plan = (slug: string) => ({
    slug,
    path: `doc/plans/atlas/DONE-PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state: "done",
    pressure_date: null,
    listed: true,
  });
  return atlas({
    sections: [section("01", true), section("02", true), section("03", true)],
    plan_rows: [plan("atlas-01"), plan("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [
      offer("refine_spec", "Refine the spec"),
      offer("new_metaplan_round", "New metaplan round"),
    ],
    items: [],
  });
}

/** Sections 02 and 03 missing, 02 the primary, and a team offered: the page draws pace and queue. */
function gapsChecklist(): ChecklistView {
  const item = (number: 2 | 3) => ({
    step: 2,
    text: `section 0${number}: ${SECTION_NAMES[`0${number}`]}`,
    path: `doc/product/atlas/section_0${number}.md`,
    offer: writeSection(number),
  });
  return atlas({
    sections: [section("01", true), section("02", false), section("03", false)],
    metaplans: [],
    plan_rows: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    untested: 0,
    current: { kind: "step", number: 2 },
    steps: checklistFixture.checklists[0]!.steps.map((step) =>
      step.number === 2 ? { ...step, mark: "gap", count: "1/3" } : step,
    ),
    primary: writeSection(2),
    secondary: [writeSection(3), offer("draft_all_sections", "Draft all")],
    items: [item(2), item(3)],
  });
}

function seededStore(checklist: ChecklistView, active: string | null): Store {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(activeFeatureAtom, active);
  return store;
}

function renderRoutes(path: string, checklist: ChecklistView) {
  const store = seededStore(checklist, null);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { router, store };
}

function renderHeader(checklist: ChecklistView): HTMLElement {
  render(
    <Provider store={seededStore(checklist, "atlas")}>
      <MemoryRouter initialEntries={["/f/atlas/product"]}>
        <PlanHeader onOpenSettings={() => {}} onOpenHelp={() => {}} />
      </MemoryRouter>
    </Provider>,
  );
  return screen.getByRole("banner");
}

function spine(): HTMLElement {
  return screen.getByRole("navigation", { name: SPINE });
}

function offerButtons(root: HTMLElement, label: string): HTMLElement[] {
  return within(root)
    .queryAllByRole("button")
    .filter((button) => button.textContent?.includes(label));
}

function queueRow(queue: HTMLElement, name: string): HTMLElement {
  const row = within(queue)
    .getAllByRole("listitem")
    .find((candidate) => candidate.textContent?.includes(name));
  if (!row) throw new Error(`no queue row for ${name}`);
  return row;
}

beforeEach(() => {
  window.localStorage.clear();
  stubMedia(true);
  stubServer();
  unregister = [
    registerOfferAction("write_section", () => {}),
    registerOfferAction("draft_all_sections", () => {}),
  ];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

it("the Vision row opens the vision document", async () => {
  const { router } = renderRoutes("/f/atlas/product", finishedChecklist());

  const row = within(await screen.findByRole("navigation", { name: SPINE })).getByRole("link", {
    name: /\bVision\b/,
  });
  expect(row.getAttribute("href")).toBe("/f/atlas/vision");

  fireEvent.click(row);

  await waitFor(() => expect(router.state.location.pathname).toBe("/f/atlas/vision"));
  expect(router.state.location.search).toBe("");
  // The body comes from the vision's summary page, so a page naming only the path never shows it.
  const column = await screen.findByRole("document", { name: "Reading column" });
  await waitFor(() => expect(column.textContent).toContain(VISION_SENTENCE));
  expect(spine()).toBeTruthy();
});

it("no screen draws one offer twice", async () => {
  renderRoutes("/f/atlas/product", gapsChecklist());

  const pace = await screen.findByRole("group", { name: "Choose a pace" });
  const queue = screen.getByRole("list", { name: "Section queue" });

  // Pace owns the primary here, and the offers are live buttons or the counts below prove nothing.
  expect(within(pace).getAllByRole("button").length).toBeGreaterThan(0);
  const primaryRow = queueRow(queue, SECTION_NAMES["02"]);
  expect(within(primaryRow).getByText("Next")).toBeTruthy();
  expect(within(primaryRow).queryByRole("button")).toBeNull();
  const otherRow = queueRow(queue, SECTION_NAMES["03"]);
  expect(offerButtons(otherRow, "Write section 03")).toHaveLength(1);

  for (const label of ["Write section 02", "Write section 03", "Draft all"]) {
    expect(offerButtons(spine(), label)).toEqual([]);
    expect(offerButtons(document.body, label).length).toBeLessThanOrEqual(1);
  }
});

it("the header has no rail or view tabs", () => {
  const rail = '.plan-next-step, .plan-header-rail, [class*="plan-step"]';

  const wide = renderHeader(gapsChecklist());
  const links = within(wide);
  expect(links.getByRole("link", { name: "Home" })).toBeTruthy();
  expect(links.getByRole("link", { name: "Files" })).toBeTruthy();
  expect(links.queryByRole("link", { name: /\b(Product|Engineering)\b/ })).toBeNull();
  for (const name of SPINE_ROWS) expect(links.queryByRole("link", { name })).toBeNull();
  expect(links.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(wide.querySelector(rail)).toBeNull();
  cleanup();

  // Below the wide query the narrow header is checked too, on its other code path.
  stubMedia(false);
  const narrow = renderHeader(gapsChecklist());
  expect(within(narrow).queryByRole("link", { name: /\b(Product|Engineering)\b/ })).toBeNull();
  expect(narrow.querySelector(rail)).toBeNull();
});
