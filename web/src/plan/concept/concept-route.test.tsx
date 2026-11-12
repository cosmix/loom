import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";

import { conceptPageLabel } from "./concept-route";
import {
  authorWithOffers,
  canAuthor,
  clone,
  descriptionOf,
  doc,
  empty,
  FIRST_OVERVIEW,
  keyList,
  makeStubFetch,
  openFirstRun,
  OVERVIEW_RUN,
  planPage,
  readingColumn,
  renderAt,
  text,
  withOffers,
  type Page,
} from "./concept-route-support";
import { conceptPageAtom } from "./page-state";

const extras = vi.hoisted(() => ({
  header: vi.fn(() => null),
  chip: vi.fn(() => null),
  action: vi.fn(() => null),
}));
vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: extras.header,
  SummaryAction: extras.action,
  SummaryDetail: () => null,
}));
vi.mock("./key-chip-extras", () => ({ KeyChipExtras: extras.chip }));

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
let failing = false;
const fetched: string[] = [];

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    "overview/demo/product": { ...overview, key: "overview/demo/product" },
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
  extras.action.mockClear();
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing }), fetched)));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
describe("overviews and the document key", () => {
  it.each(["product"])(
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
    renderAt("/f/demo/product", (store) =>
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
  it("no Fold all shows while a sentence stays closed; the hint row names the keys that fold", async () => {
    serveOverview();
    renderAt("/f/demo/product", canAuthor);
    await openFirstRun();
    expect(screen.queryByRole("button", { name: "Fold all" })).toBeNull();
    const hints = screen.getByRole("group", { name: "Reading keys" });
    expect(Array.from(hints.querySelectorAll("kbd"), (kbd) => kbd.textContent)).toEqual([
      "Enter",
      "Esc",
      "⇧ Esc",
      "a",
    ]);
    expect(hints.textContent).toContain("adds the sentence to chat");
  });
  it("the hint row omits the add key when the browser cannot author", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    await openFirstRun();
    const hints = screen.getByRole("group", { name: "Reading keys" });
    expect(Array.from(hints.querySelectorAll("kbd"), (kbd) => kbd.textContent)).not.toContain("a");
    expect(hints.textContent).not.toContain("adds the sentence to chat");
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

describe("empty and partial states", () => {
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
    expect(conceptPageLabel("/f/demo/engineering")).toBeNull();
    expect(conceptPageLabel("/f/demo/product/section_04")).toBe("section_04 summary");
    expect(conceptPageLabel("/f/demo/engineering/feat-x")).toBe("feat-x summary");
    expect(conceptPageLabel("/plans/x")).toBe("x summary");
    expect(conceptPageLabel("/f/demo/metaplan")).toBe("Metaplan summary");
    expect(conceptPageLabel("/f/demo/metaplan")).toBe("Metaplan summary");
    expect(conceptPageLabel("/product")).toBeNull();
    expect(conceptPageLabel("/engineering/a")).toBeNull();
    expect(conceptPageLabel("/f/demo/product/a%2Fb")).toBeNull();
    expect(conceptPageLabel("/files/doc/x.md")).toBeNull();
  });
  it("PageHeaderExtras and SummaryAction mounted on a summary page, which draws no key", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(extras.header).toHaveBeenCalled();
    expect(extras.action).toHaveBeenCalled();
    expect(extras.chip).not.toHaveBeenCalled();
  });
  it("KeyChipExtras mounted in an overview's key", async () => {
    serveOverview();
    renderAt("/f/demo/product");
    await readingColumn();
    expect(extras.chip).toHaveBeenCalled();
  });
});
