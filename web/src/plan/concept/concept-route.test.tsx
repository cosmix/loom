import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { checklistAtom } from "@/plan/state/atoms";

import { conceptPageLabel } from "./concept-route";
import {
  canAuthor,
  clone,
  descriptionOf,
  doc,
  FIRST_OVERVIEW,
  keyList,
  makeStubFetch,
  openFirstRun,
  OVERVIEW_RUN,
  planPage,
  readingColumn,
  renderAt,
  text,
  type Page,
  type Store,
} from "./concept-route-support";
import { conceptPageAtom } from "./page-state";

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
describe("overviews and the document key", () => {
  it.each(["product", "engineering"])(
    "keeps unmarked %s overview sentences free of source marks",
    async (scope) => {
      const overview = conceptPageSchema.parse(overviewFixture);
      const paragraph = overview.blocks.find((block) => block.kind === "paragraph");
      if (paragraph?.kind !== "paragraph") throw new Error("Expected an overview paragraph");
      paragraph.sentences[0]!.marks = [];
      serveOverview(overview);
      renderAt(`/f/demo/${scope}`);
      const column = await readingColumn();
      const sentence = within(column)
        .getByRole("button", { name: FIRST_OVERVIEW })
        .closest(".concept-sentence-wrap");

      expect(sentence?.querySelector(".concept-mark")).toBeNull();
    },
  );
  it("engineering tab renders and keeps runs", async () => {
    serveOverview();
    renderAt("/f/demo/engineering");
    await openFirstRun();
    expect(window.sessionStorage.getItem("loom-plan:concept:overview/demo/engineering")).toContain(
      "s1",
    );
    expect(screen.getByText("Engineering overview")).toBeTruthy();
  });
  it("unlisted list links to summary pages", async () => {
    const overview = {
      ...clone(overviewFixture),
      unlisted: [doc("a", "Alpha", "demo/engineering/a")],
    };
    serveOverview(overview);
    renderAt("/f/demo/engineering");
    const section = await screen.findByRole("region", { name: "Unlisted plans" });
    expect(within(section).getByRole("link", { name: "Alpha" }).getAttribute("href")).toBe(
      "/f/demo/engineering/a",
    );
  });
  it("chip shows colour, label and title", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    await readingColumn();
    const link = within(keyList()).getByRole("link", { name: /Review/ });
    expect(link.textContent).toContain("02");
    const mark = link.querySelector<HTMLElement>('[data-kind="section"]')!;
    expect(mark.style.color).toBe("var(--doc-2)");
    expect(mark.querySelector(".lucide-book-open")).not.toBeNull();
  });
  it("chip click navigates to summary page", async () => {
    serveOverview();
    const { router } = renderAt("/f/demo/product");
    await readingColumn();
    fireEvent.click(within(keyList()).getByRole("link", { name: /Review/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/product/section_02"));
  });
  it("plan chip tooltip lists Implements", async () => {
    const overview = clone(overviewFixture);
    overview.key_chips[0]!.doc = doc("a", "Plan a", "demo/engineering/a");
    serveOverview(overview);
    const links = [
      {
        number: 1,
        name: "section_01",
        doc: doc("section_01", "Planning", "demo/product/section_01"),
      },
    ];
    const loaded = conceptPageSchema.parse(
      planPage({ key: "demo/engineering/a", implements: { kind: "sections", links } }),
    );
    // The seeded page is one the server has: mounting refetches every cached page.
    pages["demo/engineering/a"] = loaded;
    renderAt("/f/demo/engineering", (store) =>
      store.set(conceptPageAtom("demo/engineering/a"), {
        page: loaded,
        previous: null,
        error: null,
      }),
    );
    await readingColumn();
    const chip = within(keyList()).getByRole("link", { name: /Plan a/ });
    expect(descriptionOf(chip)).toContain("Implements: Planning");
  });
  it("no plan in muted text", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    await readingColumn();
    expect(within(keyList()).getByText("no plan").className).toContain("text-muted-foreground");
  });
  it("Fold all folds every run", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    const column = await openFirstRun();
    fireEvent.click(screen.getByRole("button", { name: "Fold all" }));
    await waitFor(() =>
      expect(within(column).queryByRole("group", { name: OVERVIEW_RUN })).toBeNull(),
    );
  });
  it("opening a run changes no URL and sends no request", async () => {
    serveOverview();
    const { router } = renderAt("/f/demo/product");
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
    expect(router.state.location.pathname).toBe("/f/demo/product");
    expect(fetched).toHaveLength(before);
  });
  it("tab switching keeps open runs", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    await openFirstRun();
    const tabs = screen.getByRole("navigation", { name: "Views" });
    fireEvent.click(within(tabs).getByRole("link", { name: "Engineering" }));
    await screen.findByText("Engineering overview");
    fireEvent.click(within(tabs).getByRole("link", { name: "Product" }));
    const column = await readingColumn();
    expect(await within(column).findByRole("group", { name: OVERVIEW_RUN })).toBeTruthy();
  });
  it("navigating to a cached page remounts the column without notes", async () => {
    serveOverview();
    const { router } = renderAt("/f/demo/product");
    const overview = await openFirstRun();
    await router.navigate("/f/demo/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    const summary = await readingColumn();
    expect(summary).not.toBe(overview);
    expect(overview.isConnected).toBe(false);
    expect(screen.queryAllByRole("note")).toHaveLength(0);
  });
  it("back returns to previous page", async () => {
    serveOverview();
    const { router } = renderAt("/f/demo/product");
    await readingColumn();
    fireEvent.click(within(keyList()).getByRole("link", { name: /Review/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/product/section_02"));
    await router.navigate(-1);
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/product"));
    expect(await readingColumn()).toBeTruthy();
  });
});
/** The offers plan 1 sends in the checklist; `EmptyOffer` looks its offer up there. */
function withOffers(store: Store): void {
  store.set(
    checklistAtom,
    checklistViewSchema.parse({
      ...checklistFixture.checklists[0]!,
      secondary: [
        {
          id: "start-vision",
          kind: "start_vision",
          label: "Start the product vision",
          target: null,
        },
        {
          id: "start-metaplan",
          kind: "start_metaplan",
          label: "Start a metaplan from the product spec",
          target: null,
        },
      ],
    }),
  );
}
const authorWithOffers = (store: Store): void => {
  canAuthor(store);
  withOffers(store);
};

describe("empty and partial states", () => {
  const empty = (kind: string, extra: Page = {}) => ({
    ...clone(overviewFixture),
    key_chips: [],
    blocks: [],
    empty: { kind, count: null, offer: null, docs: [], ...extra },
  });
  it("sections without vision list", async () => {
    const docsList = [doc("b", "Beta", "demo/product/b"), doc("c", "Gamma", "demo/product/c")];
    pages = { "overview/demo/product": empty("sections_no_vision", { docs: docsList }) };
    renderAt("/f/demo/product");
    const list = await screen.findByRole("list", { name: "Documents" });
    expect(
      within(list)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["Beta", "Gamma"]);
  });
  it("No metaplan yet", async () => {
    pages = {
      "overview/demo/engineering": {
        ...empty("no_metaplan", { offer: "start-metaplan" }),
        key: "overview/demo/engineering",
      },
    };
    renderAt("/f/demo/engineering", authorWithOffers);
    expect(await screen.findByText("No metaplan yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
  });
  it("plans without metaplan list and offer", async () => {
    pages = {
      "overview/demo/engineering": {
        ...empty("plans_no_metaplan", {
          docs: [doc("a", "Alpha", "demo/engineering/a")],
          offer: "start-metaplan",
        }),
        key: "overview/demo/engineering",
      },
    };
    renderAt("/f/demo/engineering", authorWithOffers);
    const list = await screen.findByRole("list", { name: "Documents" });
    expect(within(list).getByRole("link", { name: "Alpha" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
  });
  it("never built state", async () => {
    const files = [
      {
        ...doc("section_01", "Planning", "demo/product/section_01"),
        path: "doc/product/demo/section_01.md",
      },
    ];
    pages = { "overview/demo/product": empty("never_built", { count: 3, docs: files }) };
    renderAt("/f/demo/product");
    expect(await screen.findByText("Summaries not built")).toBeTruthy();
    expect(text()).toContain("3 documents");
    expect(screen.getByRole("link", { name: "Planning" }).getAttribute("href")).toBe(
      "/files/product/demo/section_01.md",
    );
  });
  it("building state with key chips", async () => {
    pages = {
      "overview/demo/product": {
        ...empty("building"),
        key_chips: clone(overviewFixture.key_chips),
      },
    };
    renderAt("/f/demo/product");
    expect((await screen.findAllByText("Building summaries")).length).toBeGreaterThan(0);
    expect(within(keyList()).getAllByRole("link")).toHaveLength(overviewFixture.key_chips.length);
  });
  it("read-only browser sees no offer button", async () => {
    pages = { "overview/demo/product": empty("no_vision", { offer: "start-vision" }) };
    renderAt("/f/demo/product", withOffers);
    expect(await screen.findByText("No product vision yet")).toBeTruthy();
    expect(screen.getByText("Start the product vision")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Start the product vision/ })).toBeNull();
  });
  it("offer button is enabled and undescribed for the holder", async () => {
    pages = { "overview/demo/product": empty("no_vision", { offer: "start-vision" }) };
    renderAt("/f/demo/product", authorWithOffers);
    const button = await screen.findByRole("button", { name: "Start the product vision" });
    expect(button.hasAttribute("aria-disabled")).toBe(false);
    expect(button.hasAttribute("disabled")).toBe(false);
    expect(descriptionOf(button)).toBe("");
  });

  it("unknown summary shows not-found with file mode link", async () => {
    docs = {
      "product/demo/section_09.md": {
        ...clone(filesDocFixture),
        path: "doc/product/demo/section_09.md",
      },
    };
    renderAt("/f/demo/product/section_09");
    expect(await screen.findByText("No summary for section_09")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open in file mode" }).getAttribute("href")).toBe(
      "/files/product/demo/section_09.md",
    );
  });

  it("fetch failure shows the error with Retry and Retry refetches", async () => {
    serveOverview();
    failing = true;
    renderAt("/f/demo/product");
    expect(await screen.findByText("database is down")).toBeTruthy();
    failing = false;
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await readingColumn()).toBeTruthy();
  });
});

describe("wiring", () => {
  it("conceptPageLabel names Product overview and section_04 summary", () => {
    expect(conceptPageLabel("/f/demo/product")).toBe("Product overview");
    expect(conceptPageLabel("/f/demo/engineering")).toBe("Engineering overview");
    expect(conceptPageLabel("/f/demo/product/section_04")).toBe("section_04 summary");
    expect(conceptPageLabel("/f/demo/engineering/feat-x")).toBe("feat-x summary");
    expect(conceptPageLabel("/plans/x")).toBe("x summary");
    expect(conceptPageLabel("/product")).toBeNull();
    expect(conceptPageLabel("/engineering/a")).toBeNull();
    expect(conceptPageLabel("/f/demo/product/a%2Fb")).toBeNull();
    expect(conceptPageLabel("/files/doc/x.md")).toBeNull();
  });

  it("PageHeaderExtras, KeyChipExtras and KeyRowExtras mounted", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(extras.header).toHaveBeenCalled();
    expect(extras.row).toHaveBeenCalled();
    expect(extras.chip).toHaveBeenCalled();
  });
});
