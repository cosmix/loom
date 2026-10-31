import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { featureOfKey } from "@/plan/api/page-key";
import { checklistViewSchema } from "@/plan/api/schema";
import { conceptPageSchema, type ConceptPage } from "@/plan/api/schema-concept";
import { authorStore, makeDoc } from "@/plan/files/doc-heading-kit";
import { checklistAtom } from "@/plan/state/atoms";

import { PageHeading } from "./page-heading";
import {
  clone,
  json,
  makeStubFetch,
  renderAt,
  planPage,
  keyList,
  readingColumn,
  text,
  type Page,
} from "./concept-route-support";

const extras = vi.hoisted(() => ({
  header: vi.fn(() => null),
  chip: vi.fn(() => null),
  row: vi.fn(() => null),
}));
vi.mock("./page-header-extras", () => ({ PageHeaderExtras: extras.header }));
vi.mock("./key-chip-extras", () => ({ KeyChipExtras: extras.chip }));
vi.mock("./key-row-extras", () => ({ KeyRowExtras: extras.row }));

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
let failing = false;
const fetched: string[] = [];

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    "overview/demo/product": { ...overview, key: "overview/demo/product" },
    "overview/demo/engineering": { ...overview, key: "overview/demo/engineering" },
    "demo/product/section_01": clone(summaryFixture),
    "demo/product/section_02": {
      ...clone(summaryFixture),
      key: "demo/product/section_02",
      title: "Review",
    },
  };
}
beforeEach(() => {
  pages = {};
  docs = {};
  failing = false;
  fetched.length = 0;
  extras.header.mockClear();
  extras.chip.mockClear();
  extras.row.mockClear();
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing }), fetched)));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
describe("summary page header", () => {
  it("header shows title, path, colour, state and source view", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    const title = await screen.findByRole("heading", { level: 1, name: "Planning" });
    const header = title.closest("header") as HTMLElement;
    expect(within(header).queryByText("doc/product/demo/section_01.md")).toBeNull();
    const crumbs = within(header).getByRole("navigation", { name: "Breadcrumb" });
    expect(within(crumbs).getAllByRole("listitem")[0]?.textContent).toBe("demo");
    expect(within(crumbs).getByRole("link", { name: "demo" }).getAttribute("href")).toBe("/f/demo");
    expect((title.previousElementSibling as HTMLElement).style.color).toBe("var(--doc-2)");
    expect(within(header).getByText("Out of date: the source changed")).toBeTruthy();
    const link = within(header).getByRole("link", { name: "Source" });
    expect(link.getAttribute("href")).toBe("/files/product/demo/section_01.md");
    expect(extras.header).toHaveBeenCalled();
  });
  it("plan header shows state and pressure date", async () => {
    pages = {
      "demo/engineering/a": planPage({ key: "demo/engineering/a", last_pressure: "2026-10-20" }),
    };
    renderAt("/f/demo/engineering/a");
    await screen.findByRole("heading", { level: 1, name: "Plan a" });
    expect(screen.getByText("Written").closest("[data-chip]")).not.toBeNull();
    expect(text()).toContain("Pressure-tested 2026-10-20");
  });
  it("locked notice on a done plan", async () => {
    pages = { "demo/engineering/a": planPage({ key: "demo/engineering/a", plan_state: "done" }) };
    renderAt("/f/demo/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain("This plan is done. Record changes in a new plan.");
  });
  it("locked notice on a running plan", async () => {
    pages = {
      "demo/engineering/a": planPage({ key: "demo/engineering/a", plan_state: "running" }),
    };
    renderAt("/f/demo/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain(
      "This plan is running. loom run works from the committed copy, so the app keeps it read-only.",
    );
    expect(within(note).getByText("loom run", { selector: "code" })).toBeTruthy();
  });
  it("Implemented by with states", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(text()).toContain("Built by");
    expect(
      screen.getByRole("link", { name: "05 (Concept mode, in progress)" }).getAttribute("href"),
    ).toBe("/f/demo/engineering/feat-planner-05-concept-mode");
    expect(text()).toContain("in progress)");
  });
  it("the Open all header button opens the page's runs", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    const column = await readingColumn();
    expect(within(column).queryAllByRole("group", { name: /^Detail of/ })).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Open all" }));
    const runs = await within(column).findAllByRole("group", { name: /^Detail of/ });
    expect(runs.length).toBeGreaterThan(0);
  });
});

describe("summary page header source actions", () => {
  const sourceOf = (stem: string) => `doc/product/demo/${stem}.md`;
  const pageOf = (stem: string): ConceptPage =>
    conceptPageSchema.parse({
      ...clone(summaryFixture),
      state: null,
      implemented_by: [],
      feature: featureOfKey(summaryFixture.key),
      path: sourceOf(stem),
    });

  it("never lends the previous page's file to Edit and Discuss while the next one loads", async () => {
    const files = { editable: true, lock: null } as const;
    const docA = makeDoc({ ...files, path: sourceOf("stale_a"), hash: "sha256:a" });
    const docB = makeDoc({ ...files, path: sourceOf("stale_b"), hash: "sha256:b" });
    let release: (response: Response) => void = () => {};
    const held = new Promise<Response>((resolve) => {
      release = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const href = input instanceof URL ? input.href : input;
        const url = new URL(typeof href === "string" ? href : href.url, "http://localhost");
        const wanted = url.searchParams.get("path");
        if (url.pathname !== "/api/doc") return Promise.resolve(json({ error: "not_found" }, 404));
        return wanted === "product/demo/stale_a.md" ? Promise.resolve(json(docA)) : held;
      }),
    );
    const store = authorStore();
    initClient(store);
    const heading = (stem: string) => (
      <Provider store={store}>
        <MemoryRouter>
          <PageHeading page={pageOf(stem)} />
        </MemoryRouter>
      </Provider>
    );
    const { rerender } = render(heading("stale_a"));
    const edit = await screen.findByRole("link", { name: "Edit" });
    expect(edit.getAttribute("href")).toBe("/files/product/demo/stale_a.md?edit=1");

    rerender(heading("stale_b"));

    expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();
    expect((screen.getByRole("button", { name: "Edit" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Discuss" }) as HTMLButtonElement).disabled).toBe(
      true,
    );

    release(json(docB));
    await waitFor(() =>
      expect(screen.getByRole("link", { name: "Edit" }).getAttribute("href")).toBe(
        "/files/product/demo/stale_b.md?edit=1",
      ),
    );
  });
});

describe("document key across pages", () => {
  /** An overview of six chips, labelled `<prefix>01` to `<prefix>06`. */
  const longKey = (key: string, prefix: string) => {
    const overview = { ...clone(overviewFixture), key };
    overview.key_chips = Array.from({ length: 6 }, (_, index) => {
      const label = String(index + 1).padStart(2, "0");
      const chip = clone(overviewFixture.key_chips[0]!);
      chip.doc = { ...chip.doc, id: `section_${label}`, page: `demo/product/section_${label}` };
      return { ...chip, label: `${prefix}${label}` };
    });
    return overview;
  };

  it("opens collapsed on the next loaded page after the key was expanded", async () => {
    pages = {
      "overview/demo/product": longKey("overview/demo/product", "p"),
      "overview/demo/engineering": longKey("overview/demo/engineering", "e"),
    };
    const { router } = renderAt("/f/demo/engineering");
    await screen.findByText("e01");
    // The product overview is loaded in turn, so the way back finds the engineering page loaded.
    await router.navigate("/f/demo/product");
    await screen.findByText("p01");
    fireEvent.click(screen.getByRole("button", { name: /^…\s?2 more$/ }));
    expect(within(keyList()).getAllByRole("link")).toHaveLength(6);
    expect(screen.getByRole("button", { name: "Fewer" })).toBeTruthy();

    await router.navigate("/f/demo/engineering");

    await screen.findByText("e01");
    expect(within(keyList()).getAllByRole("link")).toHaveLength(4);
    expect(screen.queryByRole("button", { name: "Fewer" })).toBeNull();
    expect(screen.getByRole("button", { name: /^…\s?2 more$/ })).toBeTruthy();
  });

  it("keeps focus on a chip link after navigating to a loaded page with the same chip", async () => {
    // Same first four chips, but the engineering page has five, so its "more" chip reads "1 more".
    const engineering = longKey("overview/demo/engineering", "k");
    engineering.key_chips = engineering.key_chips.slice(0, 5);
    pages = {
      "overview/demo/product": longKey("overview/demo/product", "k"),
      "overview/demo/engineering": engineering,
    };
    const { router } = renderAt("/f/demo/engineering");
    const more = (count: number) => screen.findByRole("button", { name: `…${count} more` });
    await more(1);
    // Each page is loaded in turn, so the later visits find their page loaded.
    await router.navigate("/f/demo/product");
    await more(2);
    await router.navigate("/f/demo/engineering");
    await more(1);
    const link = within(keyList()).getAllByRole("link")[0]!;
    link.focus();
    expect(document.activeElement).toBe(link);

    await router.navigate("/f/demo/product");

    await more(2);
    expect(document.activeElement).toBe(within(keyList()).getAllByRole("link")[0]);
  });
});

describe("overview eyebrow", () => {
  it("never shows a slug holding a format character: the route refuses it before any eyebrow", async () => {
    const slug = "dem\u202Eo";
    // Served so that a loosened slug rule would load the overview and reach the eyebrow.
    const key = `overview/${slug}/engineering`;
    pages = { [key]: { ...clone(overviewFixture), key } };
    renderAt(`/f/${encodeURIComponent(slug)}/engineering`, (store) =>
      store.set(
        checklistAtom,
        checklistViewSchema.parse({ ...checklistFixture.checklists[0]!, feature: slug }),
      ),
    );
    await screen.findByText("No such document");
    expect(screen.queryByRole("document", { name: "Reading column" })).toBeNull();
    expect(text()).not.toContain("\u202E");
    expect(fetched.some((url) => url.includes("/api/concept/"))).toBe(false);
  });
});

describe("overview head", () => {
  it("the overview eyebrow names the feature by its slug", async () => {
    serveOverview();
    renderAt("/f/demo/engineering", (store) =>
      store.set(checklistAtom, checklistViewSchema.parse(checklistFixture.checklists[0]!)),
    );
    await readingColumn();
    expect(screen.getByText("demo · engineering overview")).toBeTruthy();
  });
  it("an overview draws its eyebrow, then its page title, then the gaps above its text", async () => {
    serveOverview();
    renderAt("/f/demo/product", (store) =>
      store.set(checklistAtom, checklistViewSchema.parse(checklistFixture.checklists[0]!)),
    );
    const column = await readingColumn();
    const eyebrow = document.querySelector(".plan-eyebrow")!;
    const title = screen.getByRole("heading", { level: 1, name: "Product overview" });
    expect(eyebrow.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(title.compareDocumentPosition(column) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
