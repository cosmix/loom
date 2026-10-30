import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { PLANNER_SHORTCUT_GROUPS } from "@/plan/help/shortcuts";
import { activeFeatureAtom } from "@/plan/state/atoms";

import { ConceptRoute } from "./concept-route";
import {
  canAuthor,
  clone,
  json,
  makeStubFetch,
  openFirstRun,
  OVERVIEW_RUN,
  planPage,
  readingColumn,
  renderAt,
  stubWideHeader,
} from "./concept-route-support";
import { conceptPageAtom } from "./page-state";
import { openRunsAtom } from "./view-state";

let pages: Record<string, unknown> = {};
const fetched: string[] = [];
const requested = (prefix: string) => fetched.filter((url) => url.startsWith(prefix));

function serveOverview(): void {
  pages = {
    "overview/demo/product": clone(overviewFixture),
    "overview/demo/engineering": { ...clone(overviewFixture), key: "overview/demo/engineering" },
    "demo/product/section_01": clone(summaryFixture),
    "demo/product/section_02": { ...clone(summaryFixture), key: "demo/product/section_02" },
  };
}

/** Mount directly: the application router deliberately has no legacy concept routes. */
function renderConceptAt(path: string) {
  const store = createStore();
  initClient(store);
  store.set(conceptPageAtom(overviewFixture.key), {
    page: conceptPageSchema.parse(overviewFixture),
    previous: null,
    error: null,
  });
  const router = createMemoryRouter([{ path: "*", element: <ConceptRoute /> }], {
    initialEntries: [path],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, router };
}

beforeEach(() => {
  pages = {};
  fetched.length = 0;
  window.sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), fetched)),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  window.getSelection()?.removeAllRanges();
  Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
});

describe("addresses that are not page keys", () => {
  it.each([
    "/product",
    "/f/demo/product/..%2F..%2Fserver",
    "/f/demo/product/%2E%2E",
    "/f/demo/engineering/a%2Fb",
  ])("%s shows not-found and sends no request", async (path) => {
    pages = { "overview/demo/product": clone(overviewFixture) };
    renderConceptAt(path);
    expect(await screen.findByText(/does not exist/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(requested("/api/concept")).toEqual([]);
    expect(requested("/api/doc")).toEqual([]);
  });
});

describe("a 400 bad_page answer", () => {
  it("shows not-found with no Retry", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json({ error: "bad_page", message: "not a concept page key" }, 400)),
    );
    renderAt("/f/demo/product/section_01");
    expect(await screen.findByText("section_01 does not exist.")).toBeTruthy();
    expect(screen.queryByText("Could not load this page")).toBeNull();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });
});

describe("selection spanning sentences", () => {
  function selectSentences(column: HTMLElement, from: string, to: string): void {
    const first = column.querySelector(`[data-sentence-id="${from}"]`)!.firstChild as Text;
    const last = column.querySelector(`[data-sentence-id="${to}"]`)!.firstChild as Text;
    const range = document.createRange();
    range.setStart(first, 0);
    range.setEnd(last, last.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    act(() => void document.dispatchEvent(new Event("selectionchange")));
  }

  it("Add to chat attaches every selected sentence, not only the first", async () => {
    // jsdom does not implement layout for ranges.
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    pages = { "overview/demo/product": clone(overviewFixture) };
    const { store } = renderAt("/f/demo/product", canAuthor);
    const column = await readingColumn();
    selectSentences(column, "s1", "s3");
    fireEvent.click(await screen.findByRole("button", { name: "Add to chat" }));
    const sentences = store
      .get(pendingAttachmentsAtom)
      .map((draft) => (draft.kind === "summary" ? draft.sentence : draft.kind));
    expect(sentences).toEqual(["s1", "s2", "s3"]);
    expect(window.getSelection()?.rangeCount).toBe(0);
  });
});

describe("a selection whose later part is over the attachment limit", () => {
  const SECTION = "doc/product/demo/section_01.md";
  let docs: Record<string, unknown> = {};

  beforeEach(() => {
    docs = {};
    vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing: false }), fetched)));
  });

  /** Section 01 with a short Intent and a Stages section whose body is `stages`. */
  function sectionDoc(stages: string) {
    const text = `# Planning\n\n## Intent\n\nA plan begins with intent.\n\n## Stages\n\n${stages}\n`;
    const sections = [
      { level: 2, title: "Intent", anchor: "intent", start_line: 3, end_line: 6 },
      { level: 2, title: "Stages", anchor: "stages", start_line: 7, end_line: 10 },
    ];
    const base = clone(filesDocFixture);
    return { ...base, path: SECTION, title: "Planning", text, sections, size: text.length };
  }

  function lastTextNode(root: Element): Text {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let last: Text | null = null;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) last = node as Text;
    return last!;
  }

  /** Opens sentence s2 to its Stages passage and selects from s1 to the passage's end. */
  async function selectSentenceAndPassage(stages: string, loaded: RegExp) {
    // jsdom does not implement layout for ranges.
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    pages = { "demo/product/section_01": clone(summaryFixture) };
    docs = { "product/demo/section_01.md": sectionDoc(stages) };
    const { store } = renderAt("/f/demo/product/section_01", canAuthor);
    const column = await readingColumn();
    fireEvent.click(within(column).getByRole("button", { name: /^Each stage names/ }));
    await within(column).findByText(loaded);
    const first = column.querySelector('[data-sentence-id="s1"]')!.firstChild as Text;
    const last = lastTextNode(column.querySelector("[data-passage]")!);
    const range = document.createRange();
    range.setStart(first, 0);
    range.setEnd(last, last.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    act(() => void document.dispatchEvent(new Event("selectionchange")));
    return store;
  }

  it("adds the sentence and a passage within the limit", async () => {
    const store = await selectSentenceAndPassage("Stages list their files.", /^Stages list/);
    fireEvent.click(await screen.findByRole("button", { name: "Add to chat" }));
    await waitFor(() => expect(window.getSelection()?.rangeCount).toBe(0));
    expect(store.get(pendingAttachmentsAtom).map((draft) => draft.kind)).toEqual([
      "summary",
      "source",
    ]);
  });

  it("shows the over-limit note and adds nothing when a passage is over 16 KiB", async () => {
    const huge = `Marker ${"filler ".repeat(3000)}`;
    const store = await selectSentenceAndPassage(huge, /^Marker filler/);
    expect(await screen.findByText(/Selection is over 16 KiB/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
    expect(store.get(pendingAttachmentsAtom)).toEqual([]);
  });
});

describe("keys", () => {
  it("a adds an attachment while canAuthor", async () => {
    serveOverview();
    const { store } = renderAt("/f/demo/product", canAuthor);
    const column = await readingColumn();
    column.focus();
    await waitFor(() => expect(column.contains(document.activeElement)).toBe(true));
    fireEvent.keyDown(column, { key: "a" });
    await waitFor(() => expect(store.get(pendingAttachmentsAtom)).toHaveLength(1));
  });
  it("a does nothing while another browser holds authoring", async () => {
    serveOverview();
    const { store } = renderAt("/f/demo/product");
    const column = await readingColumn();
    column.focus();
    fireEvent.keyDown(column, { key: "a" });
    expect(store.get(pendingAttachmentsAtom)).toHaveLength(0);
  });
  it("g p and g e", async () => {
    serveOverview();
    const { router } = renderAt("/f/demo/product");
    await readingColumn();
    fireEvent.keyDown(document.body, { key: "g" });
    fireEvent.keyDown(document.body, { key: "e" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/engineering"));
    fireEvent.keyDown(document.body, { key: "g" });
    fireEvent.keyDown(document.body, { key: "p" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/f/demo/product"));
  });
  it("g p and g e do nothing on a standalone plan", async () => {
    pages = { "plans/x": planPage({ key: "plans/x", feature: null }) };
    const { router } = renderAt("/plans/x");
    await readingColumn();
    for (const key of ["g", "p", "g", "e"]) fireEvent.keyDown(document.body, { key });
    expect(router.state.location.pathname).toBe("/plans/x");
  });
  it("help shortcuts hold the reading group with eleven rows", () => {
    const group = PLANNER_SHORTCUT_GROUPS.find((entry) => entry.title === "While reading");
    expect(group?.rows).toHaveLength(11);
  });
});

describe("route keyboard and feature guards", () => {
  async function openTwoRuns() {
    stubWideHeader();
    serveOverview();
    const { store } = renderAt("/f/demo/product");
    const column = await openFirstRun();
    fireEvent.click(within(column).getByRole("button", { name: /^The product exists/ }));
    expect(store.get(openRunsAtom("overview/demo/product")).map((run) => run.sentenceId)).toEqual([
      "s1",
      "s2",
    ]);
    const button = screen.getByRole("button", { name: "Settings" });
    act(() => button.focus());
    return { store, column, button, runs: openRunsAtom("overview/demo/product") };
  }

  it("Escape on a header button folds the last opened run and keeps header focus", async () => {
    const { store, button, runs } = await openTwoRuns();
    fireEvent.keyDown(button, { key: "Escape" });
    expect(store.get(runs).map((run) => run.sentenceId)).toEqual(["s1"]);
    expect(document.activeElement).toBe(button);
  });

  it("Shift+Escape on a header button folds every run", async () => {
    const { store, button, runs } = await openTwoRuns();
    fireEvent.keyDown(button, { key: "Escape", shiftKey: true });
    expect(store.get(runs)).toEqual([]);
  });

  it.each([false, true])(
    "Escape inside an open dialog preserves runs (shift=%s)",
    async (shiftKey) => {
      const { store, runs } = await openTwoRuns();
      const open = store.get(runs);
      const dialog = document.createElement("div");
      dialog.setAttribute("role", "dialog");
      dialog.setAttribute("data-state", "open");
      const close = document.createElement("button");
      close.textContent = "Close";
      dialog.append(close);
      document.body.append(dialog);
      try {
        close.focus();
        fireEvent.keyDown(close, { key: "Escape", shiftKey });
        expect(store.get(runs)).toEqual(open);
      } finally {
        dialog.remove();
      }
    },
  );

  it("Escape in the column is folded only once by its own handler", async () => {
    const { store, column, runs } = await openTwoRuns();
    const first = within(column).getByRole("group", { name: OVERVIEW_RUN });
    act(() => first.focus());
    fireEvent.keyDown(first, { key: "Escape" });
    expect(store.get(runs).map((run) => run.sentenceId)).toEqual(["s2"]);
  });

  it.each(["altKey", "ctrlKey", "metaKey"])("Escape with %s preserves runs", async (modifier) => {
    const { store, button, runs } = await openTwoRuns();
    const open = store.get(runs);
    fireEvent.keyDown(button, { key: "Escape", [modifier]: true });
    expect(store.get(runs)).toEqual(open);
  });

  it("Escape while typing preserves runs", async () => {
    const { store, runs } = await openTwoRuns();
    const open = store.get(runs);
    const input = document.createElement("input");
    document.body.append(input);
    try {
      input.focus();
      fireEvent.keyDown(input, { key: "Escape" });
      expect(store.get(runs)).toEqual(open);
    } finally {
      input.remove();
    }
  });

  it("the shown feature updates activeFeatureAtom on navigation", async () => {
    serveOverview();
    pages["other/product/section_01"] = {
      ...clone(summaryFixture),
      key: "other/product/section_01",
      feature: "other",
    };
    const { store, router } = renderAt("/f/demo/product", (state) => {
      state.set(activeFeatureAtom, "old");
    });
    await readingColumn();
    expect(store.get(activeFeatureAtom)).toBe("demo");
    await act(() => router.navigate("/f/other/product/section_01"));
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(store.get(activeFeatureAtom)).toBe("other");
  });
});

describe("unlisted documents", () => {
  it.each([
    ["overview/demo/product", "/f/demo/product", "Unlisted sections"],
    ["demo/product/section_01", "/f/demo/product/section_01", null],
    ["plans/x", "/plans/x", null],
  ] as const)("%s lists unlisted documents only on an overview", async (key, path, heading) => {
    const unlisted = [
      { id: "x", title: "Standalone", path: "doc/plans/PLAN-x.md", page: "plans/x" },
    ];
    pages = { [key]: { ...clone(heading ? overviewFixture : summaryFixture), key, unlisted } };
    renderAt(path);
    await readingColumn();
    if (heading) {
      const region = screen.getByRole("region", { name: heading });
      expect(within(region).getByRole("link", { name: "Standalone" }).getAttribute("href")).toBe(
        "/plans/x",
      );
    } else {
      expect(screen.queryByRole("region", { name: /Unlisted/ })).toBeNull();
    }
  });
});
