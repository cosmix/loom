import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, useAtomValue } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import { pageHref } from "@/plan/api/page-key";
import type { ConceptPage, Mark, Sentence } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";

import { conceptPageAtom } from "./page-state";
import { ReadingColumn } from "./reading-column";
import { cite, docOf, pageOf, S1, S2, sent, src } from "./reading-column-fixtures";

/**
 * Mounting and driving the reading column in tests. `/api/doc` serves `docs`; the cited
 * summary pages the column prefetches are served from `pages`, so a page a test placed in
 * the store is not wiped by a 404 for it.
 */
export const docs: Record<string, unknown> = {};
export const pages: Record<string, ConceptPage> = {};

/** Server-shaped document links for the column's synthetic pages. */
export const docLink = (stem: string, title: string) => ({
  id: stem,
  title,
  path: `doc/product/${stem}.md`,
  page: `demo/product/${stem}`,
});
export const mark = (
  stem: string,
  title: string,
  colour: number,
  out: Mark["out_of_date"] = null,
): Mark => ({ doc: docLink(stem, title), colour, out_of_date: out });

const CONCEPT = "/api/concept/";

function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(
    typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
    "http://x",
  );
  const doc = url.pathname === "/api/doc" ? docs[url.searchParams.get("path") ?? ""] : undefined;
  const page = url.pathname.startsWith(CONCEPT) ? pages[url.pathname.slice(CONCEPT.length)] : null;
  const body = doc ?? page ?? { error: "not_found", message: "not found" };
  return Promise.resolve(new Response(JSON.stringify(body), { status: doc || page ? 200 : 404 }));
}

/** Places a summary page in the store and serves it to the column's prefetch. */
export function loadPage(store: ReturnType<typeof createStore>, key: string, page: ConceptPage) {
  pages[key] = page;
  act(() => store.set(conceptPageAtom(key), { page, previous: null, error: null }));
}

function Harness({
  pageKey,
  onAttach,
}: {
  pageKey: string;
  onAttach?: (d: AttachmentDraft) => void;
}) {
  const page = useAtomValue(conceptPageAtom(pageKey)).page;
  return page ? <ReadingColumn pageKey={pageKey} page={page} onAttach={onAttach} /> : null;
}

export function mount(pageKey: string, page: ConceptPage, onAttach?: (d: AttachmentDraft) => void) {
  const store = createStore();
  initClient(store);
  store.set(conceptPageAtom(pageKey), { page, previous: null, error: null });
  const router = createMemoryRouter(
    [{ path: "*", element: <Harness pageKey={pageKey} onAttach={onAttach} /> }],
    { initialEntries: [pageHref(pageKey)] },
  );
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, router, column: screen.getByRole("document", { name: "Reading column" }) };
}

/** Mounts the column on a product overview made of one paragraph of `sentences`. */
export const mountProduct = (sentences: Sentence[], extra: Record<string, unknown> = {}) =>
  mount("overview/demo/product", pageOf("overview/demo/product", sentences, extra));

export function replace(store: ReturnType<typeof createStore>, key: string, next: ConceptPage) {
  act(() => {
    const page = store.get(conceptPageAtom(key)).page;
    store.set(conceptPageAtom(key), { page: next, previous: page, error: null });
  });
}

/** An overview whose sentence "o" cites sentence "x" of section_01, loaded in the store. */
export function nested(extraOverview: Partial<Sentence> = {}) {
  const o = {
    ...sent("o", "Overview opens.", [cite("section_01", "x")], [mark("section_01", "Planning", 1)]),
    ...extraOverview,
  };
  const overview = pageOf("overview/demo/product", [o]);
  const detail = pageOf("demo/product/section_01", [
    sent("x", "Detail opens.", [src(S1, "intro")]),
  ]);
  pages["demo/product/section_01"] = detail;
  const view = mount("overview/demo/product", overview);
  loadPage(view.store, "demo/product/section_01", detail);
  return { ...view, overview, detail };
}

export const press = (key: string) =>
  fireEvent.keyDown(document.activeElement ?? document.body, { key });
export const active = () => document.activeElement as HTMLElement;
export const button = (column: HTMLElement, name: RegExp | string) =>
  within(column).getByRole("button", { name });
export const group = (column: HTMLElement) =>
  within(column).findByRole("group", { name: /^Detail of/ });
export const two = [src(S2, "intro"), src(S2, "detail")];
export const pair = () => [sent("a", "Stays put."), sent("b", "Goes away.")];
export const flashed = (column: HTMLElement, text: string) =>
  within(column).getByText(text).className.includes("concept-flash");

/** Registers the per-test setup: default documents, empty session storage, stubbed fetch. */
export function installColumnTestHooks(): void {
  beforeEach(() => {
    for (const key of Object.keys(docs)) delete docs[key];
    for (const key of Object.keys(pages)) delete pages[key];
    docs["product/section_02.md"] = docOf(S2);
    docs["product/section_01.md"] = docOf(S1);
    window.sessionStorage.clear();
    vi.stubGlobal("fetch", vi.fn(stubFetch));
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });
}
