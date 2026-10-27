import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import { pendingAttachmentsAtom } from "@/plan/attachments";

import {
  canAuthor,
  clone,
  json,
  makeStubFetch,
  readingColumn,
  renderAt,
} from "./concept-route-support";

let pages: Record<string, unknown> = {};
const fetched: string[] = [];
const requested = (prefix: string) => fetched.filter((url) => url.startsWith(prefix));

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
  it.each(["/product/..%2F..%2Fserver", "/product/%2E%2E", "/engineering/a%2Fb"])(
    "%s shows not-found and sends no request",
    async (path) => {
      pages = { product: clone(overviewFixture), engineering: clone(overviewFixture) };
      renderAt(path);
      expect(await screen.findByText(/does not exist/)).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
      expect(requested("/api/concept")).toEqual([]);
      expect(requested("/api/doc")).toEqual([]);
    },
  );
});

describe("a 400 bad_page answer", () => {
  it("shows not-found with no Retry", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json({ error: "bad_page", message: "not a concept page key" }, 400)),
    );
    renderAt("/product/section_01");
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
    pages = { product: clone(overviewFixture) };
    const { store } = renderAt("/product", canAuthor);
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
  const SECTION = "doc/product/section_01.md";
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
    pages = { "product/section_01": clone(summaryFixture) };
    docs = { "product/section_01.md": sectionDoc(stages) };
    const { store } = renderAt("/product/section_01", canAuthor);
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
