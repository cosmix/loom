import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, MemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  serverInfoSchema,
  type ChecklistView,
} from "@/plan/api/schema";
import { chatSnapshotSchema, type Turn } from "@/plan/api/schema-chat";
import { traceViewSchema, type TraceDoc, type TraceView } from "@/plan/api/schema-trace";
import { TurnFooter } from "@/plan/chat/turn-footer";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));
// The footer's restore controls need a held session; these contracts read the rows beside them.
vi.mock("@/plan/chat/restore", () => ({ RestoreButton: () => null, RestoreAll: () => null }));

const SPINE = "Feature documents";
const WIDE_QUERY = "(min-width: 1024px)";
const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";
const S03 = "doc/product/demo/section_03.md";
const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const SECOND_PLAN = "doc/plans/demo/PLAN-second.md";
/** The opened document's save hash: the source view and the summary page both carry it. */
const HASH = "sha256:5e771ed0c0ffee";
const SETTLED_COMMIT = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";

type Call = { url: URL; method: string };
type Answer = { status: number; body: unknown };

let calls: Call[] = [];
let trace: TraceView;
let settleAnswers: Answer[] = [];

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function requestUrl(input: RequestInfo | URL): URL {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost");
}

/** The fixture trace, with section 03 written so it is a draft summary page like any other. */
function baseTrace(): TraceView {
  const view = traceViewSchema.parse(structuredClone(traceFixture));
  patchDoc(view, S03, { progress: "written" });
  return view;
}

function patchDoc(view: TraceView, path: string, patch: Partial<TraceDoc>): void {
  const doc = view.documents.find((candidate) => candidate.path === path);
  if (!doc) throw new Error(`the trace fixture holds no ${path}`);
  Object.assign(doc, patch);
}

function sentence(id: string, text: string, path: string, anchor: string) {
  const at = { path, anchor };
  return { id, text, marks: [], citations: [{ kind: "source", ...at }], rests_on: [at], plans: [] };
}

/** A section summary whose two sentences rest on its headings `one` and `two`. */
function summaryPage(key: string, path: string): unknown {
  const sentences = [
    sentence("s1", "Intent is written down before anything is built.", path, "one"),
    sentence("s2", "Each stage names the files it may change.", path, "two"),
  ];
  const blocks = [summaryFixture.blocks[0], { kind: "paragraph", sentences }];
  return {
    ...structuredClone(summaryFixture),
    key,
    path,
    feature: "demo",
    file_hash: HASH,
    blocks,
  };
}

function conceptPages(): Record<string, unknown> {
  const pages: Record<string, unknown> = {
    "overview/demo/product": {
      ...overviewFixture,
      key: "overview/demo/product",
      feature: "demo",
      path: "doc/product/demo/index.md",
    },
  };
  for (const path of [S01, S02, S03]) {
    const key = `demo/product/${path.split("/").pop()!.replace(/\.md$/, "")}`;
    pages[key] = summaryPage(key, path);
  }
  return pages;
}

function answer(url: URL, method: string): Response {
  const { pathname } = url;
  if (pathname === "/api/tree") return json(treeFixture);
  if (pathname === "/api/trace/demo") return json(trace);
  if (pathname === "/api/settle" && method === "POST") {
    const next = settleAnswers.shift();
    return next
      ? json(next.body, next.status)
      : json({ result: "saved", hash: "sha256:after", validation: null });
  }
  if (pathname === "/api/settled-version") return json({ commit: SETTLED_COMMIT });
  if (pathname === "/api/doc") {
    return json({ ...docFixture, path: `doc/${url.searchParams.get("path")}`, hash: HASH });
  }
  if (pathname === "/api/implemented-by") return json({ sections: {} });
  if (pathname === "/api/diff/turn") {
    return json({ path: `doc/${url.searchParams.get("path")}`, before: "old\n", after: "new\n" });
  }
  const pages = conceptPages();
  const key = decodeURIComponent(pathname.replace("/api/concept/", ""));
  if (pathname.startsWith("/api/concept/") && key in pages) return json(pages[key]);
  return json({ error: "not_found", message: "no such page" }, 404);
}

function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = requestUrl(input);
    const method = init?.method ?? (input instanceof Request ? input.method : "GET");
    calls.push({ url, method });
    return Promise.resolve(answer(url, method));
  });
}

function settlePosts(): URLSearchParams[] {
  return calls
    .filter((call) => call.method === "POST" && call.url.pathname === "/api/settle")
    .map((call) => call.url.searchParams);
}

function stubMedia(): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === WIDE_QUERY,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

function section(number: string, name: string) {
  return { number, name, written: true, open_questions: 0 };
}

/** The demo feature as the trace fixture describes it: three written sections and plan first. */
function demoChecklist(): ChecklistView {
  const base = checklistFixture.checklists[0]!;
  return checklistViewSchema.parse({
    ...base,
    sections: [section("01", "Alpha"), section("02", "Beta"), section("03", "Gamma")],
    plan_rows: [{ ...base.plan_rows[0]!, slug: "first", path: FIRST_PLAN, title: "First" }],
  });
}

function storeWithProject() {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [demoChecklist()], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  return store;
}

function renderRoutes(path: string): void {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={storeWithProject()}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

/** The closed sentence's wrap: its text, then whatever the page draws after it. */
async function sentenceWrap(id: string): Promise<HTMLElement> {
  await screen.findByRole("document", { name: "Reading column" });
  return waitFor(() => {
    const element = document.querySelector<HTMLElement>(`[data-sentence-id="${id}"]`);
    if (!element?.parentElement) throw new Error(`sentence ${id} is not drawn`);
    return element.parentElement;
  });
}

beforeEach(() => {
  window.localStorage.clear();
  calls = [];
  settleAnswers = [];
  trace = baseTrace();
  stubMedia();
  stubServer();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("the spine shows each document's settle state", async () => {
  renderRoutes("/f/demo/product");
  const spine = within(await screen.findByRole("navigation", { name: SPINE }));

  // Section 01 is settled in the trace, section 03 a draft.
  await spine.findByRole("link", {
    name: (name) => /\bAlpha\b/.test(name) && /\bSettled\b/.test(name) && !/\bDraft\b/.test(name),
  });
  await spine.findByRole("link", {
    name: (name) => /\bGamma\b/.test(name) && /\bDraft\b/.test(name) && !/\bSettled\b/.test(name),
  });
});

it("Mark settled posts the document and its hash", async () => {
  renderRoutes("/f/demo/product/section_03");

  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));

  await waitFor(() => expect(settlePosts()).toHaveLength(1));
  const [params] = settlePosts();
  expect(params!.get("path")).toBe("product/demo/section_03.md");
  expect(params!.get("hash")).toBe(HASH);
  expect(params!.get("confirm")).toBeNull();
});

it("an unresolved answer asks before settling with confirm=1", async () => {
  settleAnswers = [
    {
      status: 409,
      body: { error: "unresolved", message: "unresolved points", my_calls: 1, open_questions: 0 },
    },
  ];
  renderRoutes("/f/demo/product/section_03");

  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  const dialog = await screen.findByRole("dialog");
  expect(dialog.textContent).toMatch(/\b(1|one) My call\b/i);
  // Nothing is resent until the reader confirms.
  expect(settlePosts()).toHaveLength(1);
  expect(settlePosts()[0]!.get("confirm")).toBeNull();

  fireEvent.click(within(dialog).getByRole("button", { name: /settle/i }));

  await waitFor(() => expect(settlePosts()).toHaveLength(2));
  const resent = settlePosts()[1]!;
  expect(resent.get("confirm")).toBe("1");
  expect(resent.get("path")).toBe("product/demo/section_03.md");
  expect(resent.get("hash")).toBe(HASH);
});

it("a revised document links to its changes since settled", async () => {
  // Section 02 reads `revised` in the trace fixture.
  renderRoutes("/f/demo/product/section_02");

  const link = await screen.findByRole("link", { name: "Changes since settled" });
  expect(link.getAttribute("href")).toBeTruthy();
});

it("frontier hints are at most five links", async () => {
  // Seven documents, each a gap of its own, with titles nothing else on the page draws.
  trace.documents.forEach((doc, index) => {
    doc.title = `Frontier doc ${index + 1}`;
  });
  trace.frontier = trace.documents.slice(0, 7).map((doc) => ({
    kind: "settle" as const,
    node: { path: doc.path, anchor: null },
    count: null,
  }));
  expect(trace.frontier).toHaveLength(7);
  renderRoutes("/f/demo/product");
  await screen.findByRole("navigation", { name: SPINE });

  const hints = () => screen.queryAllByRole("link", { name: /Frontier doc \d/ });
  await waitFor(() => expect(hints().length).toBeGreaterThan(0));
  expect(hints()).toHaveLength(5);
});

it("a sentence shows the plans covering the headings it rests on", async () => {
  // Heading `one` is covered by plan first; heading `two` of the same section by plan second only.
  trace.headings = trace.headings.map((heading) =>
    heading.path === S01 && heading.anchor === "two"
      ? {
          ...heading,
          covered_by: [
            { kind: "plan", path: SECOND_PLAN, anchor: null, slug: "second", stage: null },
          ],
        }
      : heading,
  );
  renderRoutes("/f/demo/product/section_01");

  const restsOnOne = await sentenceWrap("s1");
  const restsOnTwo = await sentenceWrap("s2");
  await waitFor(() => expect(restsOnOne.textContent).toMatch(/\bfirst\b/));
  expect(restsOnOne.textContent).not.toMatch(/\bsecond\b/);
  await waitFor(() => expect(restsOnTwo.textContent).toMatch(/\bsecond\b/));
  expect(restsOnTwo.textContent).not.toMatch(/\bfirst\b/);
});

function changedTurn(path: string): Turn {
  const fixture = chatSnapshotSchema.parse(snapshot).turns[0]!;
  return {
    ...fixture,
    id: "t2",
    open: false,
    changed_files: [{ path, before_hash: "before", by_tool: true }],
    summaries_rebuilding: [],
  };
}

it("the turn footer lists settled plans downstream of a changed section", async () => {
  // Plan first covers section 01's headings and its spec moved since it was settled.
  patchDoc(trace, FIRST_PLAN, { settle: "upstream_changed" });
  render(
    <Provider store={storeWithProject()}>
      <MemoryRouter>
        <TurnFooter turn={changedTurn(S01)} sessionId="session one" />
      </MemoryRouter>
    </Provider>,
  );

  const footer = await screen.findByRole("group", { name: "Turn 2 changes" });
  const [chip] = await within(footer).findAllByText("section_01.md");
  const item = chip!.closest("li");
  if (!item) throw new Error("the changed section has no row of its own");
  await waitFor(() => expect(item.textContent).toMatch(/upstream changed/i));
  expect(item.textContent).toMatch(/\bfirst\b/i);
});
