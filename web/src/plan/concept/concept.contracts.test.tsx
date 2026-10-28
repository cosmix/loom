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
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { authoringViewSchema, checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { routes } from "@/plan/router";
import { authoringAtom, checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

type Store = ReturnType<typeof createStore>;
type Anchor = { path: string; anchor: string; title?: string };
type OpenRun = { level: 0 | 1 | 2; sentenceId: string; restsOn: Anchor[]; parent: string | null };
type FoldNote = { paragraphOf: string; text: string; path: string };
type Page = Record<string, unknown>;
type OpenStateModule = {
  carryOpenState(prev: Page, next: Page, open: OpenRun[]): { open: OpenRun[]; notes: FoldNote[] };
};

const SECTION_01 = "doc/product/section_01.md";
const SECTION_02 = "doc/product/section_02.md";
const FIRST_OVERVIEW = /^Teams plan work together/;
const OVERVIEW_RUN = "Detail of Teams plan work together and";

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};

function clone<T>(value: T): T {
  return structuredClone(value);
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

// Answers /api/concept/<key> from `pages` and /api/doc?path= from `docs`, keyed by the
// doc-relative path; anything else is a 404 so a stray request cannot pass for data.
function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  const url = new URL(raw, "http://localhost");
  const notFound = jsonResponse({ error: "not_found", message: "not found" }, 404);
  if (url.pathname.startsWith("/api/concept/")) {
    const page = pages[decodeURIComponent(url.pathname.slice("/api/concept/".length))];
    return Promise.resolve(page ? jsonResponse(page) : notFound);
  }
  if (url.pathname === "/api/doc") {
    const doc = docs[(url.searchParams.get("path") ?? "").replace(/^doc\//, "")];
    return Promise.resolve(doc ? jsonResponse(doc) : notFound);
  }
  return Promise.resolve(notFound);
}

function sentence(id: string, text: string, restsOn: Anchor[]) {
  const citations = restsOn.map(({ path, anchor }) => ({ kind: "source", path, anchor }));
  return { id, text, marks: [], citations, rests_on: restsOn, plans: [] };
}

function summaryPage(key: string, sentences: ReturnType<typeof sentence>[]): Page {
  const heading = { kind: "heading", level: 1, text: "Planning" };
  return { ...clone(summaryFixture), key, blocks: [heading, { kind: "paragraph", sentences }] };
}

function reviewSummary(): Page {
  const page = summaryPage("product/section_02", [
    sentence("s2", "Reviewers read each plan before it runs.", [
      { path: SECTION_02, anchor: "port" },
    ]),
  ]);
  return { ...page, title: "Review", path: SECTION_02, colour: 2, key_chips: [] };
}

function section01Doc() {
  const text =
    "# Planning\n\n## Intent\n\nA plan begins with **intent** written down.\n\n" +
    "## Stages\n\nStages list their files.\n";
  const sections = [
    { level: 2, title: "Intent", anchor: "intent", start_line: 3, end_line: 6 },
    { level: 2, title: "Stages", anchor: "stages", start_line: 7, end_line: 10 },
  ];
  const base = clone(filesDocFixture);
  return { ...base, path: SECTION_01, title: "Planning", text, sections, size: text.length };
}

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    product: overview,
    "product/section_01": clone(summaryFixture),
    "product/section_02": reviewSummary(),
  };
}

function renderAt(path: string, setup?: (store: Store) => void) {
  const store = createStore();
  initClient(store);
  setup?.(store);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const view = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { router, view };
}

function readingColumn(): Promise<HTMLElement> {
  return screen.findByRole("document", { name: "Reading column" });
}

// The roving focus may move DOM focus or point at its target with aria-activedescendant.
function focusTarget(): Element | null {
  const active = document.activeElement;
  const id = active?.getAttribute("aria-activedescendant");
  return id ? document.getElementById(id) : active;
}

function press(key: string): void {
  fireEvent.keyDown(document.activeElement ?? document.body, { key });
}

function marksIn(root: Element): Element[] {
  return Array.from(root.querySelectorAll("[aria-label]")).filter((element) =>
    /^[^:]+: doc\/\S+\.md/.test(element.getAttribute("aria-label") ?? ""),
  );
}

function isDashed(element: Element): boolean {
  return (
    element.closest('[data-dashed="true"]') !== null ||
    element.querySelector('[data-dashed="true"]') !== null
  );
}

async function loadOpenState(): Promise<OpenStateModule> {
  // A variable specifier keeps vite from resolving the module at collection, so a missing
  // open-state.ts fails inside the contracts that call it instead of hiding every other one.
  const specifier = "./open-state";
  return (await import(/* @vite-ignore */ specifier)) as OpenStateModule;
}

beforeEach(() => {
  pages = {};
  docs = {};
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(stubFetch));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("concept opening an overview sentence shows its cited summary sentences in place", async () => {
  serveOverview();
  const { router } = renderAt("/product");
  const column = await readingColumn();
  const next = within(column).getByRole("button", { name: /^The product exists/ });
  fireEvent.click(within(column).getByRole("button", { name: FIRST_OVERVIEW }));
  const run = await within(column).findByRole("group", { name: OVERVIEW_RUN });
  expect(router.state.location.pathname).toBe("/product");
  // In place: the run sits before the paragraph's next sentence, not after the paragraph.
  expect(run.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const opened = Array.from(run.querySelectorAll("[aria-expanded]"));
  expect(opened.map((element) => element.textContent?.trim().slice(0, 18))).toEqual([
    "A plan starts from",
    "Reviewers read eac",
  ]);
  const labels = marksIn(run).map((mark) => mark.getAttribute("aria-label") ?? "");
  expect(labels).toHaveLength(2);
  expect(labels[0]).toMatch(/^Planning: doc\/product\/section_01\.md/);
  expect(labels[1]).toMatch(/^Review: doc\/product\/section_02\.md/);
});

it("concept opening a summary sentence shows its source passages as inset blocks", async () => {
  pages = { "product/section_01": clone(summaryFixture) };
  docs = { "product/section_01.md": section01Doc() };
  renderAt("/product/section_01");
  const column = await readingColumn();
  const next = within(column).getByRole("button", { name: /^Each stage names the files/ });
  fireEvent.click(within(column).getByRole("button", { name: /^A plan starts from/ }));
  const header = await within(column).findByRole("link", { name: /section_01\.md.*Intent/ });
  const href = header.getAttribute("href") ?? "";
  expect(href.startsWith("/files/product/section_01.md")).toBe(true);
  expect(href).toContain("intent");
  const strong = await within(column).findByText("intent", { selector: "strong" });
  let block: HTMLElement | null = header.parentElement;
  while (block && !block.contains(strong)) block = block.parentElement;
  expect(block).not.toBeNull();
  expect(block!.tagName).not.toBe("SPAN");
  expect(block!.closest("p")).toBeNull();
  expect(block!.contains(next)).toBe(false);
  expect(block!.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(column.textContent).not.toContain("**intent**");
  expect(column.textContent).not.toContain("Stages list their files.");
});

it("concept keyboard down arrow enters an opened run and escape folds it back", async () => {
  serveOverview();
  renderAt("/product");
  const column = await readingColumn();
  column.focus();
  await waitFor(() => expect(focusTarget()?.textContent).toMatch(FIRST_OVERVIEW));
  press("Enter");
  const run = await within(column).findByRole("group", { name: OVERVIEW_RUN });
  await within(run).findByText(/Reviewers read each plan/);
  press("ArrowDown");
  await waitFor(() => {
    const target = focusTarget();
    expect(target?.textContent).toMatch(/^A plan starts from/);
    expect(run.contains(target)).toBe(true);
  });
  press("Escape");
  await waitFor(() => {
    expect(within(column).queryByRole("group", { name: OVERVIEW_RUN })).toBeNull();
    const target = focusTarget();
    expect(target?.isConnected).toBe(true);
    expect(target?.textContent).toMatch(FIRST_OVERVIEW);
  });
  expect(column.contains(document.activeElement)).toBe(true);
});

it("concept stale summary draws dashed marks and a dashed key chip", async () => {
  const overview = clone(overviewFixture);
  const stale = { reason: "source_changed", detail: null };
  overview.key_chips[1]!.out_of_date = stale as never;
  const first = overview.blocks[1] as (typeof overview.blocks)[1] & { sentences: unknown[] };
  const marks = (first.sentences[0] as { marks: { out_of_date: unknown }[] }).marks;
  marks[1]!.out_of_date = stale;
  serveOverview(overview);
  renderAt("/product");
  const column = await readingColumn();
  const sentenceButton = within(column).getByRole("button", { name: FIRST_OVERVIEW });
  expect(sentenceButton.getAttribute("aria-expanded")).toBe("false");
  expect(sentenceButton.textContent).toContain("Teams plan work together");
  const review = marksIn(column).find((mark) =>
    mark.getAttribute("aria-label")?.startsWith(`Review: ${SECTION_02}`),
  );
  expect(review?.getAttribute("aria-label")).toMatch(/, out of date: \S/);
  expect(review?.getAttribute("data-dashed")).toBe("true");
  const vision = marksIn(column).find((mark) =>
    mark.getAttribute("aria-label")?.startsWith("Vision: doc/product/vision.md"),
  );
  expect(vision?.getAttribute("data-dashed")).not.toBe("true");
  const key = screen.getByRole("list", { name: "Document key" });
  expect(isDashed(within(key).getByRole("link", { name: /Review/ }))).toBe(true);
  expect(isDashed(within(key).getByRole("link", { name: /Release/ }))).toBe(false);
});

it("concept open run moves to the new sentence with the largest source overlap", async () => {
  const anchor = (name: string): Anchor => ({ path: SECTION_01, anchor: name });
  const [a, b, c, d] = [anchor("a"), anchor("b"), anchor("c"), anchor("d")];
  const prev = summaryPage("product/section_01", [sentence("s1", "Old text.", [a, b, c])]);
  // The id-keyed and first-overlap rules both pick s1; the overlap rule picks s9.
  const next = summaryPage("product/section_01", [
    sentence("s1", "Rewritten on a.", [a]),
    sentence("s9", "Rewritten on a, b, c and d.", [a, b, c, d]),
  ]);
  const open: OpenRun[] = [{ level: 1, sentenceId: "s1", restsOn: [a, b, c], parent: null }];
  const { carryOpenState } = await loadOpenState();
  const result = carryOpenState(prev, next, open);
  expect(result.open).toHaveLength(1);
  expect(result.open[0]).toMatchObject({ level: 1, sentenceId: "s9", parent: null });
  expect(result.open[0]!.restsOn.map((entry) => entry.anchor)).toEqual(["a", "b", "c", "d"]);
  expect(result.notes).toEqual([]);
});

it("concept open run folds with a removed section note when no sentence rests on it", async () => {
  const port: Anchor = { path: SECTION_02, anchor: "port", title: "Port" };
  const other: Anchor = { path: SECTION_02, anchor: "other" };
  const prev = summaryPage("product/section_02", [
    sentence("s1", "Ports and more.", [{ path: SECTION_02, anchor: "port" }, other]),
  ]);
  const next = summaryPage("product/section_02", [sentence("s1", "Only more.", [other])]);
  const open: OpenRun[] = [
    {
      level: 1,
      sentenceId: "s1",
      restsOn: [{ path: SECTION_02, anchor: "port" }, other],
      parent: null,
    },
    { level: 2, sentenceId: `${SECTION_02}#port`, restsOn: [port], parent: "s1" },
  ];
  const { carryOpenState } = await loadOpenState();
  const result = carryOpenState(prev, next, open);
  const onPort = result.open.filter(
    (run) => run.level === 2 && run.restsOn.some((entry) => entry.anchor === "port"),
  );
  expect(onPort).toEqual([]);
  expect(result.notes).toHaveLength(1);
  expect(result.notes[0]!.text).toContain("Port");
  expect(result.notes[0]!.text).toContain("section_02.md");
  expect(result.notes[0]!.path).toContain("section_02.md");
});

it("concept plan page shows implements not stated and broken section names", async () => {
  const plan = {
    ...summaryPage("engineering/a", [sentence("s1", "Plan a builds the review.", [])]),
    title: "Plan a",
    path: "doc/plans/PLAN-a.md",
    colour: null,
    plan_state: "written",
    key_chips: [],
    implemented_by: [],
  };
  pages = { "engineering/a": { ...plan, implements: { kind: "not_stated" } } };
  const first = renderAt("/engineering/a");
  await readingColumn();
  const text = () => (document.body.textContent ?? "").replace(/\s+/g, " ");
  await waitFor(() => expect(text()).toMatch(/Implements:\s*not stated/));
  first.view.unmount();
  const links = [
    { number: 1, name: "section_01", doc: clone(summaryFixture.key_chips[0]!.doc) },
    { number: 99, name: "section_99", doc: null },
  ];
  pages = { "engineering/a": { ...plan, implements: { kind: "sections", links } } };
  const second = renderAt("/engineering/a");
  await readingColumn();
  await waitFor(() => expect(text()).toContain("Implements:"));
  const broken = screen.getAllByText(/section_99/);
  expect(broken.length).toBeGreaterThan(0);
  expect(broken.every((element) => element.closest("a") === null)).toBe(true);
  expect(second.view.container.querySelector('a[href="/product/section_01"]')).not.toBeNull();
});

it("concept sentence text with markup renders as text", async () => {
  const overview = clone(overviewFixture);
  const markup = "<img src=x onerror=alert(1)>";
  const first = overview.blocks[1] as (typeof overview.blocks)[1] & { sentences: unknown[] };
  (first.sentences[0] as { text: string }).text = `${markup} Teams plan work together.`;
  serveOverview(overview);
  const { view } = renderAt("/product");
  const column = await readingColumn();
  await within(column).findByText(/The product exists to keep intent/);
  expect(view.container.querySelector("img")).toBeNull();
  expect(column.textContent).toContain(`${markup} Teams plan work together.`);
});

it("concept view state survives a reload through session storage", async () => {
  serveOverview();
  const first = renderAt("/product");
  const column = await readingColumn();
  fireEvent.click(within(column).getByRole("button", { name: FIRST_OVERVIEW }));
  await within(column).findByRole("group", { name: OVERVIEW_RUN });
  first.view.unmount();
  expect(screen.queryByRole("document", { name: "Reading column" })).toBeNull();
  renderAt("/product");
  const reloaded = await readingColumn();
  expect(await within(reloaded).findByRole("group", { name: OVERVIEW_RUN })).toBeTruthy();
});

it("concept product route renders the product overview from the planning router", async () => {
  serveOverview();
  renderAt("/product");
  const column = await readingColumn();
  expect(
    await within(column).findByText(/Teams plan work together and review it before it ships\./),
  ).toBeTruthy();
  const key = screen.getByRole("list", { name: "Document key" });
  expect(within(key).getAllByRole("link")).toHaveLength(overviewFixture.key_chips.length);
});

it("concept empty project shows no product vision yet", async () => {
  const empty = { kind: "no_vision", count: null, offer: "start-vision", docs: [] };
  pages = { product: { ...clone(overviewFixture), key_chips: [], blocks: [], empty } };
  renderAt("/product", (store) => {
    store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
    store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
    store.set(
      checklistAtom,
      checklistViewSchema.parse({
        ...checklistFixture,
        secondary: [
          {
            id: "start-vision",
            kind: "start_vision",
            label: "Start the product vision",
            target: null,
          },
        ],
      }),
    );
  });
  expect(await screen.findByText("No product vision yet")).toBeTruthy();
  const offer = screen.getByRole("button", { name: "Start the product vision" });
  expect(offer.hasAttribute("aria-disabled")).toBe(false);
  expect(offer.hasAttribute("disabled")).toBe(false);
});
