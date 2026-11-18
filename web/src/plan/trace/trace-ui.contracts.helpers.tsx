import { render, waitFor, within, screen, cleanup } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { traceViewSchema, type TraceDoc, type TraceView } from "@/plan/api/schema-trace";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

const SPINE = "Feature documents";
export const S01 = "doc/product/demo/section_01.md";
export const S03 = "doc/product/demo/section_03.md";
export const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const HASH = "sha256:5e771ed0c0ffee";
const DEMO = { feature: "demo" };
export const SCOPE = "Every plan traces back to a written intent.";
/** The vision's scope table: section 03's row is its stub. */
const VISION_TEXT = `# Demo\n\n## Specification sections\n\n| # | Section | Covers |\n| --- | --- | --- |\n| 3 | Gamma | ${SCOPE} |\n`;
const SECTION_TEXT = "# Section\n\n## One\nOne body text.\n\n## Two\nTwo body text.\n";
export const HEADING_NAME = /\b(One|Two|Three|Four|Five|Six)\b/;
export const UNCOVERED = /\b(Five|Six)\b/;
const JSON_HEADERS = { "content-type": "application/json" };

export let trace: TraceView;
export let checklist: ReturnType<typeof demoChecklist>;

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });

export const patchDoc = (path: string, patch: Partial<TraceDoc>) =>
  Object.assign(
    trace.documents.find((doc) => doc.path === path)!,
    patch,
  );

function sentence(id: string, text: string, anchor: string) {
  const at = { path: S01, anchor };
  return { id, text, marks: [], citations: [{ kind: "source", ...at }], rests_on: [at], plans: [] };
}

/** An overview, or a summary whose two sentences rest on section 01's headings one and two. */
function conceptPage(key: string): unknown {
  const [first, , last] = key.split("/");
  const dir = key.includes("/product") ? "product" : "plans";
  if (first === "overview")
    return { ...overviewFixture, ...DEMO, key, path: `doc/${dir}/demo/index.md` };
  const path = dir === "product" ? `doc/product/demo/${last}.md` : FIRST_PLAN;
  const sentences = [
    sentence("s1", "Intent is written down before anything is built.", "one"),
    sentence("s2", "Each stage names the files it may change.", "two"),
  ];
  const blocks = [summaryFixture.blocks[0], { kind: "paragraph", sentences }];
  return { ...structuredClone(summaryFixture), ...DEMO, key, path, file_hash: HASH, blocks };
}

function h2(title: string, anchor: string, start_line: number, end_line: number) {
  return { level: 2, title, anchor, start_line, end_line };
}

/** The vision serves its scope table; every other file serves headings One and Two. */
function docView(rel: string): unknown {
  const vision = rel === "product/demo/index.md";
  const sections = vision
    ? [h2("Specification sections", "specification-sections", 3, 8)]
    : [h2("One", "one", 3, 5), h2("Two", "two", 6, 8)];
  const text = vision ? VISION_TEXT : SECTION_TEXT;
  return { ...docFixture, path: `doc/${rel}`, hash: HASH, text, sections };
}

const PAGE_KEY =
  /^(overview\/demo\/product|demo\/product\/section_0[123]|demo\/engineering\/first)$/;

function answer(url: URL, method: string): Response {
  const { pathname } = url;
  const key = decodeURIComponent(pathname.replace("/api/concept/", ""));
  // The only posts these contracts make are a settle and a finding status, both saved; the
  // settle's answer also names its queued commit.
  if (method === "POST") {
    const saved = { result: "saved", hash: "sha256:after", validation: null };
    const queued = { state: "pending", job: "3f9a1c2e-1" };
    return json(pathname === "/api/settle" ? { ...saved, commit: queued } : saved);
  }
  if (pathname === "/api/tree") return json(treeFixture);
  if (pathname === "/api/trace/demo") return json(trace);
  if (pathname === "/api/settled-version") return json({ commit: "0a1b2c3d4e5f6071" });
  if (pathname === "/api/doc") return json(docView(url.searchParams.get("path") ?? ""));
  if (pathname === "/api/implemented-by") return json({ sections: {} });
  if (pathname === "/api/run/state") return json(runStateFixture);
  if (pathname.startsWith("/api/concept/") && PAGE_KEY.test(key)) return json(conceptPage(key));
  return json({ error: "not_found", message: "no such page" }, 404);
}

function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const method = init?.method ?? (input instanceof Request ? input.method : "GET");
    return Promise.resolve(answer(new URL(raw, "http://localhost"), method));
  });
  // An EventTarget carries the media list's addEventListener and removeEventListener.
  const media = (query: string) => ({ media: query, matches: query === "(min-width: 1024px)" });
  vi.stubGlobal("matchMedia", (query: string) => Object.assign(new EventTarget(), media(query)));
}

/** The demo feature as the trace fixture describes it: three written sections and plan first. */
function demoChecklist() {
  const base = checklistFixture.checklists[0]!;
  const written = { written: true, open_questions: 0 };
  const section = (name: string, i: number) => ({ ...written, name, number: `0${i + 1}` });
  return checklistViewSchema.parse({
    ...base,
    sections: ["Alpha", "Beta", "Gamma"].map(section),
    plan_rows: [{ ...base.plan_rows[0]!, slug: "first", path: FIRST_PLAN, title: "First" }],
  });
}

export function renderWithProject(ui: Parameters<typeof render>[0]): void {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  render(<Provider store={store}>{ui}</Provider>);
}

const memory = (path: string) => createMemoryRouter(routes, { initialEntries: [path] });
export const renderRoutes = (path: string) =>
  renderWithProject(<RouterProvider router={memory(path)} />);

export const norm = (text: string | null) => (text ?? "").replace(/\s+/g, " ").trim();
export const follows = (first: Node, second: Node) =>
  (first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;

export function deepestHolding(root: Element, ...parts: string[]): HTMLElement[] {
  const holds = (el: Element) => parts.every((part) => norm(el.textContent).includes(part));
  const all = [...root.querySelectorAll<HTMLElement>("*")];
  return all.filter((el) => holds(el) && ![...el.children].some(holds));
}

export async function found<T>(find: () => T | null | undefined, what: string): Promise<T> {
  return waitFor(() => {
    const value = find();
    if (value === undefined || value === null) throw new Error(`not drawn: ${what}`);
    return value;
  });
}

export const holding = (...parts: string[]) =>
  found(() => deepestHolding(document.body, ...parts)[0], parts.join(" "));
export const spineNav = () => screen.findByRole("navigation", { name: SPINE });
export const hrefIs = (href: string) => (link: HTMLElement) => link.getAttribute("href") === href;

/** The spine's row linking `href`: the last such link, after any the spine head draws. */
export async function spineRow(href: string): Promise<HTMLElement> {
  const nav = await spineNav();
  return found(() => within(nav).queryAllByRole("link").filter(hrefIs(href)).at(-1), href);
}

export async function pageLink(name: (name: string) => boolean, href: string): Promise<void> {
  const nav = await spineNav();
  const links = () => screen.queryAllByRole("link", { name }).filter(hrefIs(href));
  await found(() => links().find((link) => !nav.contains(link)), href);
}

/** Coverage row toggles (buttons with `aria-expanded`) and tiles (with `aria-pressed`). */
const withAttr = (attr: string, name: RegExp) =>
  screen.queryAllByRole("button", { name }).filter((button) => button.hasAttribute(attr));
export const rowButtons = (name: RegExp) => withAttr("aria-expanded", name);
export const tile = (name: RegExp) => withAttr("aria-pressed", name)[0]!;
export const pressed = () =>
  [/All headings/, /Not covered/].map((name) => tile(name).getAttribute("aria-pressed"));

/**
 * Registers the contracts' per-test setup: fresh trace and checklist fixtures, the stubbed
 * server and browser, and cleanup. `trace` and `checklist` are live bindings the setup replaces,
 * so a test mutates them in place after it runs.
 */
export function useContractFixtures(): void {
  beforeEach(() => {
    window.localStorage.clear();
    // Section 03 is written, so it is a draft summary page like any other.
    trace = traceViewSchema.parse(structuredClone(traceFixture));
    patchDoc(S03, { progress: "written", settleable: true });
    checklist = demoChecklist();
    stubServer();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });
}
