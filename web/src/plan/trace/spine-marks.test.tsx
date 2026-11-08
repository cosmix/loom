import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

const ROW_GLYPH = "[data-glyph]";

/** The spine row linking `href`. */
function rowAt(spine: ReturnType<typeof renderSpine>, href: string): HTMLElement {
  const row = spine.getAllByRole("link").find((link) => link.getAttribute("href") === href);
  if (!row) throw new Error(`no spine row links ${href}`);
  return row;
}

/** What a row's glyph says: its kind and the word read after the name. */
function glyphOf(row: HTMLElement): { kind: string | null; word: string | undefined } {
  return {
    kind: row.querySelector(ROW_GLYPH)?.getAttribute("data-glyph") ?? null,
    // The roundel carries a live region of its own inside the glyph; the word is read outside it.
    word: [...row.querySelectorAll(".sr-only")].find((node) => !node.closest(ROW_GLYPH))
      ?.textContent,
  };
}

it("gives each row one glyph, chosen from the settle state, with its word read", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();

  const vision = spine.getByRole("link", { name: /^Vision/ });
  await waitFor(() => expect(glyphOf(vision).word).toBe("Settled"));
  expect(glyphOf(vision).kind).toBe("settled");
  expect(glyphOf(spine.getByRole("link", { name: /^Metaplan/ }))).toEqual({
    kind: "draft",
    word: "Draft",
  });
  const section = rowAt(spine, "/f/atlas/product/section_01");
  expect(glyphOf(section)).toEqual({ kind: "attention", word: "Changed since you settled it" });
  expandPlans(spine);
  for (const row of spine.getAllByRole("link")) {
    expect(row.querySelectorAll(ROW_GLYPH).length).toBeLessThanOrEqual(1);
  }
  // The pressure flask is gone from rows; only the words stay, read aloud.
  expect(document.querySelector(".plan-spine__badge")).toBeNull();
});

it("shows the glyph's word as its hover title and hides the glyph from assistive technology", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();

  const row = rowAt(spine, "/f/atlas/product/section_01");
  await waitFor(() => expect(glyphOf(row).kind).toBe("attention"));
  const glyph = row.querySelector<HTMLElement>(ROW_GLYPH)!;
  expect(glyph.getAttribute("title")).toBe("Changed since you settled it");
  expect(glyph.getAttribute("aria-hidden")).toBe("true");
});

it("draws the roundel on a running plan, with the plan's own state ahead of its settle state", async () => {
  // The trace calls plan atlas-01 `upstream_changed`, but it is running.
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();
  await waitFor(() =>
    expect(glyphOf(rowAt(spine, "/f/atlas/product/section_01")).kind).toBe("attention"),
  );

  expandPlans(spine);
  const running = rowAt(spine, "/f/atlas/engineering/atlas-01");
  expect(glyphOf(running)).toEqual({ kind: "running", word: "Running" });
  expect(running.querySelector(`${ROW_GLYPH} .aurora-roundel`)).not.toBeNull();
  expect(running.querySelector(".activity-glyph__led")).toBeNull();
});

it("shortens a plan's slug in its row and keeps the full slug in the row's title", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();
  expandPlans(spine);

  const row = rowAt(spine, "/f/atlas/engineering/atlas-01");
  expect(row.querySelector(".plan-spine__name")?.textContent).toBe("01");
  expect(row.getAttribute("title")).toBe("atlas-01");
});

it("marks a section that is not written with the dashed circle and no word", async () => {
  answer = { status: 200, body: atlasTrace() };
  const spine = renderSpine();
  await screen.findByRole("navigation", { name: "Feature documents" });

  const item = spine.getByText("Unwritten").closest("li")!;
  expect(item.querySelector(ROW_GLYPH)?.getAttribute("data-glyph")).toBe("not-written");
  expect(item.querySelector(".sr-only")).toBeNull();
});

it("names no draft and no settled state for done work, and still draws one glyph per row", async () => {
  const trace = atlasTrace();
  for (const doc of trace.documents) doc.settleable = doc.role === "section";
  answer = { status: 200, body: trace };
  const spine = renderSpine();

  await waitFor(() =>
    expect(glyphOf(rowAt(spine, "/f/atlas/product/section_01")).kind).toBe("attention"),
  );
  expect(glyphOf(spine.getByRole("link", { name: /^Vision/ }))).toEqual({
    kind: "draft",
    word: undefined,
  });
});

it("draws a glyph with no word for each written row while the trace is missing", async () => {
  const spine = renderSpine();
  await screen.findByRole("navigation", { name: "Feature documents" });
  await waitFor(() => expect(spine.getByRole("link", { name: /^Vision/ })).toBeTruthy());

  expect(glyphOf(spine.getByRole("link", { name: /^Vision/ }))).toEqual({
    kind: "draft",
    word: undefined,
  });
  expect(document.querySelector(".plan-spine-next")).toBeNull();
});

describe("the spine head", () => {
  const frontier: Gap[] = [
    { kind: "revised", node: { path: SECTION, anchor: null }, count: null },
    { kind: "settle", node: { path: PLAN, anchor: null }, count: null },
  ];

  it("says the top frontier item once, with one link, and nothing of the rest", async () => {
    answer = { status: 200, body: atlasTrace(frontier) };
    const spine = renderSpine();

    const head = await waitFor(() => {
      const found = document.querySelector<HTMLElement>(".plan-spine-next");
      if (!found) throw new Error("no head");
      return found;
    });
    // The head is itself the one link: verb and name on a line, the reason under it.
    expect(head.tagName).toBe("A");
    expect(head.getAttribute("href")).toBe("/f/atlas/product/section_01");
    expect(head.hasAttribute("data-look")).toBe(false);
    expect(head.querySelector(".plan-spine-next__verb")?.textContent).toBe("Review");
    expect(head.querySelector(".plan-spine-next__name")?.textContent).toBe("Run loop");
    expect(within(head).getByText("Changed since you settled it")).toBeTruthy();
    expect(within(head).queryAllByRole("link")).toHaveLength(0);
    expect(spine.getByRole("link", { name: "Review Run loop Changed since you settled it" })).toBe(
      head,
    );
    expect(spine.queryByRole("list", { name: "Needs attention" })).toBeNull();
    const groups = document.querySelector(".plan-spine__groups")!;
    expect(head.compareDocumentPosition(groups) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("draws nothing for an empty frontier", async () => {
    answer = { status: 200, body: atlasTrace() };
    const spine = renderSpine();
    await waitFor(() =>
      expect(glyphOf(spine.getByRole("link", { name: /^Vision/ })).kind).toBe("settled"),
    );

    expect(document.querySelector(".plan-spine-next")).toBeNull();
  });
});
