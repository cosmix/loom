import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { authoringViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { PLAN_HELP_KEYS } from "@/plan/help-keys";
import { routes } from "@/plan/router";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { conceptPageLabel } from "./concept-route";
import { conceptPageAtom } from "./page-state";

const extras = vi.hoisted(() => ({
  header: vi.fn(() => null),
  chip: vi.fn(() => null),
  row: vi.fn(() => null),
}));
vi.mock("./page-header-extras", () => ({ PageHeaderExtras: extras.header }));
vi.mock("./key-chip-extras", () => ({ KeyChipExtras: extras.chip }));
vi.mock("./key-row-extras", () => ({ KeyRowExtras: extras.row }));

type Store = ReturnType<typeof createStore>;
type Page = Record<string, unknown>;

const FIRST_OVERVIEW = /^Teams plan work together/;
const OVERVIEW_RUN = "Detail of Teams plan work together and";
const doc = (id: string, title: string, page: string) => ({
  id,
  title,
  path: `doc/${page}.md`,
  page,
});

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
let failing = false;
let fetched: string[] = [];

const clone = <T,>(value: T): T => structuredClone(value);
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  fetched.push(raw);
  const url = new URL(raw, "http://localhost");
  const notFound = json({ error: "not_found", message: "no such concept page" }, 404);
  if (failing && url.pathname.startsWith("/api/concept/")) {
    return Promise.resolve(json({ error: "internal", message: "database is down" }, 500));
  }
  if (url.pathname.startsWith("/api/concept/")) {
    const page = pages[decodeURIComponent(url.pathname.slice("/api/concept/".length))];
    return Promise.resolve(page ? json(page) : notFound);
  }
  if (url.pathname === "/api/doc") {
    const found = docs[(url.searchParams.get("path") ?? "").replace(/^doc\//, "")];
    return Promise.resolve(found ? json(found) : notFound);
  }
  return Promise.resolve(notFound);
}

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    product: overview,
    engineering: overview,
    "product/section_01": clone(summaryFixture),
    "product/section_02": { ...clone(summaryFixture), key: "product/section_02", title: "Review" },
  };
}

function planPage(extra: Page): Page {
  return {
    ...clone(summaryFixture),
    key: "engineering/a",
    title: "Plan a",
    path: "doc/plans/PLAN-a.md",
    colour: null,
    plan_state: "written",
    key_chips: [],
    implemented_by: [],
    implements: { kind: "not_stated" },
    ...extra,
  };
}

function canAuthor(store: Store): void {
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
}

function renderAt(path: string, setup?: (store: Store) => void) {
  const store = createStore();
  initClient(store);
  setup?.(store);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { router, store };
}

const readingColumn = () => screen.findByRole("document", { name: "Reading column" });
const text = () => (document.body.textContent ?? "").replace(/\s+/g, " ");
const keyList = () => screen.getByRole("list", { name: "Document key" });

function descriptionOf(element: Element): string {
  const ids = (element.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
  return ids.map((id) => document.getElementById(id)?.textContent?.trim() ?? "").join(" ");
}

async function openFirstRun(): Promise<HTMLElement> {
  const column = await readingColumn();
  fireEvent.click(within(column).getByRole("button", { name: FIRST_OVERVIEW }));
  await within(column).findByRole("group", { name: OVERVIEW_RUN });
  return column;
}

beforeEach(() => {
  pages = {};
  docs = {};
  failing = false;
  fetched = [];
  extras.header.mockClear();
  extras.chip.mockClear();
  extras.row.mockClear();
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(stubFetch));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("overviews and the document key", () => {
  it("engineering tab renders and keeps runs", async () => {
    serveOverview();
    renderAt("/engineering");
    await openFirstRun();
    expect(window.sessionStorage.getItem("loom-plan:concept:engineering")).toContain("s1");
    expect(screen.getByText("Engineering overview")).toBeTruthy();
  });

  it("unlisted list links to summary pages", async () => {
    const overview = { ...clone(overviewFixture), unlisted: [doc("a", "Alpha", "engineering/a")] };
    serveOverview(overview);
    renderAt("/engineering");
    const section = await screen.findByRole("region", { name: "Unlisted plans" });
    expect(within(section).getByRole("link", { name: "Alpha" }).getAttribute("href")).toBe(
      "/engineering/a",
    );
  });

  it("chip shows colour, label and title", async () => {
    serveOverview();
    renderAt("/product");
    await readingColumn();
    const link = within(keyList()).getByRole("link", { name: /Review/ });
    expect(link.textContent).toContain("02");
    const square = link.querySelector("span[aria-hidden]") as HTMLElement;
    expect(square.style.background).toContain("--doc-2");
  });

  it("chip click navigates to summary page", async () => {
    serveOverview();
    const { router } = renderAt("/product");
    await readingColumn();
    fireEvent.click(within(keyList()).getByRole("link", { name: /Review/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/product/section_02"));
  });

  it("plan chip tooltip lists Implements", async () => {
    const overview = clone(overviewFixture);
    overview.key_chips[0]!.doc = doc("a", "Plan a", "engineering/a");
    serveOverview(overview);
    const links = [
      { number: 1, name: "section_01", doc: doc("section_01", "Planning", "product/section_01") },
    ];
    const loaded = conceptPageSchema.parse(planPage({ implements: { kind: "sections", links } }));
    renderAt("/engineering", (store) =>
      store.set(conceptPageAtom("engineering/a"), { page: loaded, previous: null, error: null }),
    );
    await readingColumn();
    const chip = within(keyList()).getByRole("link", { name: /Plan a/ });
    expect(descriptionOf(chip)).toContain("Implements: Planning");
  });

  it("no plan in muted text", async () => {
    serveOverview();
    renderAt("/product");
    await readingColumn();
    expect(within(keyList()).getByText("no plan").className).toContain("text-muted-foreground");
  });

  it("Fold all folds every run", async () => {
    serveOverview();
    renderAt("/product");
    const column = await openFirstRun();
    fireEvent.click(screen.getByRole("button", { name: "Fold all" }));
    await waitFor(() =>
      expect(within(column).queryByRole("group", { name: OVERVIEW_RUN })).toBeNull(),
    );
  });

  it("opening a run changes no URL and sends no request", async () => {
    serveOverview();
    const { router } = renderAt("/product");
    const column = await readingColumn();
    await waitFor(() => expect(fetched.length).toBeGreaterThan(0));
    // Cited summary pages are prefetched on load; wait for that to settle before counting.
    let before = -1;
    await waitFor(() => {
      const settled = before === fetched.length;
      before = fetched.length;
      expect(settled).toBe(true);
    });
    fireEvent.click(within(column).getByRole("button", { name: FIRST_OVERVIEW }));
    await within(column).findByRole("group", { name: OVERVIEW_RUN });
    expect(router.state.location.pathname).toBe("/product");
    expect(fetched).toHaveLength(before);
  });

  it("tab switching keeps open runs", async () => {
    serveOverview();
    renderAt("/product");
    await openFirstRun();
    const tabs = screen.getByRole("navigation", { name: "Planning tabs" });
    fireEvent.click(within(tabs).getByRole("link", { name: "Engineering" }));
    await screen.findByText("Engineering overview");
    fireEvent.click(within(tabs).getByRole("link", { name: "Product" }));
    const column = await readingColumn();
    expect(await within(column).findByRole("group", { name: OVERVIEW_RUN })).toBeTruthy();
  });

  it("navigating to a cached page remounts the column without notes", async () => {
    serveOverview();
    const { router } = renderAt("/product");
    const overview = await openFirstRun();
    await router.navigate("/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    const summary = await readingColumn();
    expect(summary).not.toBe(overview);
    expect(overview.isConnected).toBe(false);
    expect(screen.queryAllByRole("note")).toHaveLength(0);
  });

  it("back returns to previous page", async () => {
    serveOverview();
    const { router } = renderAt("/product");
    await readingColumn();
    fireEvent.click(within(keyList()).getByRole("link", { name: /Review/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/product/section_02"));
    await router.navigate(-1);
    await waitFor(() => expect(router.state.location.pathname).toBe("/product"));
    expect(await readingColumn()).toBeTruthy();
  });
});

describe("summary page header", () => {
  it("header shows title, path, colour, state and Open in file mode", async () => {
    serveOverview();
    renderAt("/product/section_01");
    const title = await screen.findByRole("heading", { level: 1, name: "Planning" });
    const header = title.closest("header") as HTMLElement;
    expect(within(header).getByText("doc/product/section_01.md")).toBeTruthy();
    expect(header.querySelector("[data-colour='1']")).not.toBeNull();
    expect(within(header).getByText("Out of date: the source changed")).toBeTruthy();
    const link = within(header).getByRole("link", { name: "Open in file mode" });
    expect(link.getAttribute("href")).toBe("/files/product/section_01.md");
    expect(extras.header).toHaveBeenCalled();
  });

  it("plan header shows state and pressure date", async () => {
    pages = { "engineering/a": planPage({ last_pressure: "2026-10-20" }) };
    renderAt("/engineering/a");
    await screen.findByRole("heading", { level: 1, name: "Plan a" });
    expect(text()).toContain("Plan state: written");
    expect(text()).toContain("2026-10-20");
  });

  it("locked notice on a done plan", async () => {
    pages = { "engineering/a": planPage({ plan_state: "done" }) };
    renderAt("/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain("This plan is done. Record changes in a new plan.");
  });

  it("locked notice on a running plan", async () => {
    pages = { "engineering/a": planPage({ plan_state: "running" }) };
    renderAt("/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain(
      "This plan is running. `loom run` works from the committed copy.",
    );
  });

  it("Implemented by with states", async () => {
    serveOverview();
    renderAt("/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(text()).toContain("Implemented by:");
    expect(screen.getByRole("link", { name: "Concept mode" }).getAttribute("href")).toBe(
      "/engineering/feat-planner-05-concept-mode",
    );
    expect(text()).toContain("(in progress)");
  });
});

describe("keys", () => {
  it("a adds an attachment while canAuthor", async () => {
    serveOverview();
    const { store } = renderAt("/product", canAuthor);
    const column = await readingColumn();
    column.focus();
    await waitFor(() => expect(column.contains(document.activeElement)).toBe(true));
    fireEvent.keyDown(column, { key: "a" });
    await waitFor(() => expect(store.get(pendingAttachmentsAtom)).toHaveLength(1));
  });

  it("a does nothing while another browser holds authoring", async () => {
    serveOverview();
    const { store } = renderAt("/product");
    const column = await readingColumn();
    column.focus();
    fireEvent.keyDown(column, { key: "a" });
    expect(store.get(pendingAttachmentsAtom)).toHaveLength(0);
  });

  it("g p and g e", async () => {
    serveOverview();
    const { router } = renderAt("/product");
    await readingColumn();
    fireEvent.keyDown(document.body, { key: "g" });
    fireEvent.keyDown(document.body, { key: "e" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/engineering"));
    fireEvent.keyDown(document.body, { key: "g" });
    fireEvent.keyDown(document.body, { key: "p" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/product"));
  });

  it("PLAN_HELP_KEYS holds the Concept mode group with nine rows", () => {
    const group = PLAN_HELP_KEYS.find((entry) => entry.title === "Concept mode");
    expect(group?.rows).toHaveLength(9);
  });
});

describe("empty and partial states", () => {
  const empty = (kind: string, extra: Page = {}) => ({
    ...clone(overviewFixture),
    key_chips: [],
    blocks: [],
    empty: { kind, count: null, offer: null, docs: [], ...extra },
  });

  it("sections without vision list", async () => {
    const docsList = [doc("b", "Beta", "product/b"), doc("c", "Gamma", "product/c")];
    pages = { product: empty("sections_no_vision", { docs: docsList }) };
    renderAt("/product");
    const list = await screen.findByRole("list", { name: "Documents" });
    expect(
      within(list)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["Beta", "Gamma"]);
  });

  it("No metaplan yet", async () => {
    pages = { engineering: empty("no_metaplan", { offer: "start-metaplan" }) };
    renderAt("/engineering", canAuthor);
    expect(await screen.findByText("No metaplan yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
  });

  it("plans without metaplan list and offer", async () => {
    pages = {
      engineering: empty("plans_no_metaplan", {
        docs: [doc("a", "Alpha", "engineering/a")],
        offer: "start-metaplan",
      }),
    };
    renderAt("/engineering", canAuthor);
    const list = await screen.findByRole("list", { name: "Documents" });
    expect(within(list).getByRole("link", { name: "Alpha" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
  });

  it("never built state", async () => {
    const files = [doc("section_01", "Planning", "product/section_01")];
    pages = { product: empty("never_built", { count: 3, docs: files }) };
    renderAt("/product");
    expect(await screen.findByText("Summaries not built")).toBeTruthy();
    expect(text()).toContain("3 documents");
    expect(screen.getByRole("link", { name: "Planning" }).getAttribute("href")).toBe(
      "/files/product/section_01.md",
    );
  });

  it("building state with key chips", async () => {
    pages = { product: { ...empty("building"), key_chips: clone(overviewFixture.key_chips) } };
    renderAt("/product");
    expect((await screen.findAllByText("Building summaries")).length).toBeGreaterThan(0);
    expect(within(keyList()).getAllByRole("link")).toHaveLength(overviewFixture.key_chips.length);
  });

  it("read-only browser sees no offer button", async () => {
    pages = { product: empty("no_vision", { offer: "start-vision" }) };
    renderAt("/product");
    expect(await screen.findByText("No product vision yet")).toBeTruthy();
    expect(screen.getByText("Start the product vision")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Start the product vision/ })).toBeNull();
  });

  it("offer reason is the accessible description", async () => {
    pages = { product: empty("no_vision", { offer: "start-vision" }) };
    renderAt("/product", canAuthor);
    const button = await screen.findByRole("button", { name: "Start the product vision" });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(descriptionOf(button)).toBe("Available once chat is built");
  });

  it("unknown summary shows not-found with file mode link", async () => {
    docs = {
      "product/section_09.md": { ...clone(filesDocFixture), path: "doc/product/section_09.md" },
    };
    renderAt("/product/section_09");
    expect(await screen.findByText("No summary for section_09")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open in file mode" }).getAttribute("href")).toBe(
      "/files/product/section_09.md",
    );
  });

  it("fetch failure shows the error with Retry and Retry refetches", async () => {
    serveOverview();
    failing = true;
    renderAt("/product");
    expect(await screen.findByText("database is down")).toBeTruthy();
    failing = false;
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await readingColumn()).toBeTruthy();
  });
});

describe("wiring", () => {
  it("conceptPageLabel names Product overview and section_04 summary", () => {
    expect(conceptPageLabel("/product")).toBe("Product overview");
    expect(conceptPageLabel("/engineering")).toBe("Engineering overview");
    expect(conceptPageLabel("/product/section_04")).toBe("section_04 summary");
    expect(conceptPageLabel("/engineering/feat-x")).toBe("feat-x summary");
    expect(conceptPageLabel("/files/doc/x.md")).toBeNull();
  });

  it("PageHeaderExtras, KeyChipExtras and KeyRowExtras mounted", async () => {
    serveOverview();
    renderAt("/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(extras.header).toHaveBeenCalled();
    expect(extras.row).toHaveBeenCalled();
    expect(extras.chip).toHaveBeenCalled();
  });
});
