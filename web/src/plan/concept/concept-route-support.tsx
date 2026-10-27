import { fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { authoringViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { routes } from "@/plan/router";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

/** Fixtures and drivers shared by the concept route tests. */
export type Store = ReturnType<typeof createStore>;
export type Page = Record<string, unknown>;

/** What the stubbed server answers; the test file owns the values and the stub reads them. */
export type ServerState = {
  pages: Record<string, unknown>;
  docs: Record<string, unknown>;
  failing: boolean;
};

export const FIRST_OVERVIEW = /^Teams plan work together/;
export const OVERVIEW_RUN = "Detail of Teams plan work together and";

export const doc = (id: string, title: string, page: string) => ({
  id,
  title,
  path: `doc/${page}.md`,
  page,
});

export const clone = <T,>(value: T): T => structuredClone(value);
export const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** A `fetch` stand-in over `read()`; every requested URL is pushed to `fetched`. */
export function makeStubFetch(read: () => ServerState, fetched: string[]) {
  return (input: RequestInfo | URL): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    fetched.push(raw);
    const { pages, docs, failing } = read();
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
  };
}

export function planPage(extra: Page): Page {
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

export function canAuthor(store: Store): void {
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
}

export function renderAt(path: string, setup?: (store: Store) => void) {
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

export const readingColumn = () => screen.findByRole("document", { name: "Reading column" });
export const text = () => (document.body.textContent ?? "").replace(/\s+/g, " ");
export const keyList = () => screen.getByRole("list", { name: "Document key" });

export function descriptionOf(element: Element): string {
  const ids = (element.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
  return ids.map((id) => document.getElementById(id)?.textContent?.trim() ?? "").join(" ");
}

export async function openFirstRun(): Promise<HTMLElement> {
  const column = await readingColumn();
  fireEvent.click(within(column).getByRole("button", { name: FIRST_OVERVIEW }));
  await within(column).findByRole("group", { name: OVERVIEW_RUN });
  return column;
}
