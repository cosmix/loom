import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import {
  traceViewSchema,
  type DocRole,
  type Gap,
  type SettleState,
  type TraceDoc,
  type TraceView,
} from "@/plan/api/schema-trace";
import { Spine } from "@/plan/feature/spine";
import { projectAtom, serverInfoAtom } from "@/plan/state/atoms";

const SECTION = "doc/product/atlas/section_01.md";
const PLAN = "doc/plans/atlas/PLAN-atlas-01.md";
const SECOND_PLAN = "doc/plans/atlas/PLAN-atlas-02.md";

let answer: { status: number; body: unknown };

beforeEach(() => {
  window.localStorage.clear();
  answer = { status: 404, body: { error: "not_found", message: "no such feature" } };
  vi.stubGlobal("fetch", () =>
    Promise.resolve(
      new Response(JSON.stringify(answer.body), {
        status: answer.status,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function doc(path: string, role: DocRole, settle: SettleState, title: string, slug?: string) {
  const current = { body: "aaaaaaaaaaaa", stub: null, upstream: null };
  return {
    path,
    role,
    title,
    progress: "written" as const,
    settle,
    settleable: true,
    stub: null,
    number: null,
    slug: slug ?? null,
    bundle: [],
    my_calls: 0,
    open_questions: 0,
    current,
  } satisfies TraceDoc;
}

function atlasTrace(frontier: Gap[] = []): TraceView {
  return traceViewSchema.parse({
    documents: [
      doc("doc/product/atlas/index.md", "vision", "settled", "Atlas"),
      doc(SECTION, "section", "revised", "Run loop"),
      doc("doc/plans/atlas/index.md", "metaplan", "draft", "Engineering"),
      doc(PLAN, "plan", "upstream_changed", "Atlas one", "atlas-01"),
    ],
    headings: [],
    entries: [],
    findings: [],
    gaps: [],
    frontier,
    problems: [],
  });
}

function renderSpine(path = "/f/atlas/product") {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));
  const checklist = checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    sections: [
      { number: "01", name: "Run loop", written: true, open_questions: 0 },
      { number: "02", name: "Unwritten", written: false, open_questions: 0 },
    ],
    plan_rows: [
      {
        slug: "atlas-01",
        path: PLAN,
        title: "Plan one",
        state: "running",
        pressure_date: null,
        listed: true,
      },
      {
        slug: "atlas-02",
        path: SECOND_PLAN,
        title: "Plan two",
        state: "written",
        pressure_date: null,
        listed: true,
      },
    ],
  });
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <Spine /> }], {
    initialEntries: [path],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return within(screen.getByRole("navigation", { name: "Feature documents" }));
}

/** Expands the Plans group, which the route (Product spec) leaves collapsed. */
function expandPlans(spine: ReturnType<typeof renderSpine>): void {
  fireEvent.click(spine.getByRole("button", { name: "Expand Plans" }));
}

/** The settle word a spine row's link carries, read from its accessible text. */
function settleWord(link: HTMLElement): string | undefined {
  return link.querySelector<HTMLElement>(".trace-settle .sr-only")?.textContent ?? undefined;
}

it("draws each written document's settle state beside its glyphs", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();

  const vision = spine.getByRole("link", { name: /^Vision/ });
  const metaplan = spine.getByRole("link", { name: /^Metaplan/ });
  await waitFor(() => expect(settleWord(vision)).toBe("Settled"));
  expect(settleWord(metaplan)).toBe("Draft");
  expect(settleWord(spine.getByRole("link", { name: /Run loop/ }))).toBe("Revised");
  expandPlans(spine);
  const planRow = spine.getByRole("link", { name: /atlas-01/ });
  expect(settleWord(planRow)).toBe("Upstream changed");
  // The plan's own state glyph stays beside it.
  expect(planRow.textContent).toMatch(/running/);
});

it("shows the state as a hover title on a glyph, with no visible word", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();

  const row = spine.getByRole("link", { name: /Run loop/ });
  await waitFor(() => expect(settleWord(row)).toBe("Revised"));
  const glyph = row.querySelector<HTMLElement>(".trace-settle .plan-spine__glyph")!;
  expect(glyph.getAttribute("title")).toBe("Revised");
  expect(glyph.getAttribute("aria-hidden")).toBe("true");
});

it("marks no row for a document that is not written or has no trace entry", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();
  await waitFor(() =>
    expect(settleWord(spine.getByRole("link", { name: /^Vision/ }))).toBe("Settled"),
  );

  expandPlans(spine);
  // Section 02 is listed, not linked; the second plan is missing from the trace.
  expect(spine.getByText("Unwritten").closest("li")!.querySelector(".trace-settle")).toBeNull();
  expect(spine.getByRole("link", { name: /atlas-02/ }).querySelector(".trace-settle")).toBeNull();
});

it("marks no done work: a done feature's vision, metaplan and plans, while its sections keep theirs", async () => {
  const trace = atlasTrace();
  for (const doc of trace.documents) doc.settleable = doc.role === "section";
  answer = { status: 200, body: trace };
  const spine = renderSpine();

  await waitFor(() =>
    expect(settleWord(spine.getByRole("link", { name: /Run loop/ }))).toBe("Revised"),
  );
  expect(settleWord(spine.getByRole("link", { name: /^Vision/ }))).toBeUndefined();
  expect(settleWord(spine.getByRole("link", { name: /^Metaplan/ }))).toBeUndefined();
  expandPlans(spine);
  expect(spine.getByRole("link", { name: /atlas-01/ }).querySelector(".trace-settle")).toBeNull();
});

it("marks nothing while the trace is missing", async () => {
  const spine = renderSpine();
  await screen.findByRole("navigation", { name: "Feature documents" });
  await waitFor(() => expect(spine.getByRole("link", { name: /^Vision/ })).toBeTruthy());
  expect(document.querySelector(".trace-settle")).toBeNull();
  expect(spine.queryByRole("list", { name: "Needs attention" })).toBeNull();
});

it("lists the frontier hints at the head of the spine", async () => {
  const frontier: Gap[] = [
    { kind: "revised", node: { path: SECTION, anchor: null }, count: null },
    { kind: "settle", node: { path: PLAN, anchor: null }, count: null },
  ];
  answer = { status: 200, body: atlasTrace(frontier) };
  const spine = renderSpine();

  const hints = await spine.findByRole("list", { name: "Needs attention" });
  const links = within(hints).getAllByRole("link");
  expect(links.map((hint) => [hint.textContent, hint.getAttribute("href")])).toEqual([
    ["Run loop changed since you settled it", "/f/atlas/product/section_01"],
    ["Read Atlas one and settle it", "/f/atlas/engineering/atlas-01"],
  ]);
  const groups = document.querySelector(".plan-spine__groups")!;
  expect(hints.compareDocumentPosition(groups) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
