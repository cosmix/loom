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
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
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
// Offers are observed, never run: no session starts in these contracts.
vi.mock("@/plan/offers/actions", async (original) => ({
  ...(await original<typeof import("@/plan/offers/actions")>()),
  runOffer: vi.fn(() => true),
}));

const SPINE = "Feature documents";
const S01 = "doc/product/demo/section_01.md";
const S03 = "doc/product/demo/section_03.md";
const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const HASH = "sha256:5e771ed0c0ffee";
const DEMO = { feature: "demo" };
const SCOPE = "Every plan traces back to a written intent.";
/** The vision's scope table: section 03's row is its stub. */
const VISION_TEXT = `# Demo\n\n## Specification sections\n\n| # | Section | Covers |\n| --- | --- | --- |\n| 3 | Gamma | ${SCOPE} |\n`;
const SECTION_TEXT = "# Section\n\n## One\nOne body text.\n\n## Two\nTwo body text.\n";
const HEADING_NAME = /\b(One|Two|Three|Four|Five|Six)\b/;
const UNCOVERED = /\b(Five|Six)\b/;
const JSON_HEADERS = { "content-type": "application/json" };

let trace: TraceView;
let checklist: ReturnType<typeof demoChecklist>;

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });

const patchDoc = (path: string, patch: Partial<TraceDoc>) =>
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
  /^(overview\/demo\/(product|engineering)|demo\/product\/section_0[123]|demo\/engineering\/first)$/;

function answer(url: URL, method: string): Response {
  const { pathname } = url;
  const key = decodeURIComponent(pathname.replace("/api/concept/", ""));
  // The only posts these contracts make are a settle and a finding status, both saved.
  if (method === "POST") return json({ result: "saved", hash: "sha256:after", validation: null });
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

function renderWithProject(ui: Parameters<typeof render>[0]): void {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  render(<Provider store={store}>{ui}</Provider>);
}

const memory = (path: string) => createMemoryRouter(routes, { initialEntries: [path] });
const renderRoutes = (path: string) => renderWithProject(<RouterProvider router={memory(path)} />);

const norm = (text: string | null) => (text ?? "").replace(/\s+/g, " ").trim();
const follows = (first: Node, second: Node) =>
  (first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;

function deepestHolding(root: Element, ...parts: string[]): HTMLElement[] {
  const holds = (el: Element) => parts.every((part) => norm(el.textContent).includes(part));
  const all = [...root.querySelectorAll<HTMLElement>("*")];
  return all.filter((el) => holds(el) && ![...el.children].some(holds));
}

async function found<T>(find: () => T | null | undefined, what: string): Promise<T> {
  return waitFor(() => {
    const value = find();
    if (value === undefined || value === null) throw new Error(`not drawn: ${what}`);
    return value;
  });
}

const holding = (...parts: string[]) =>
  found(() => deepestHolding(document.body, ...parts)[0], parts.join(" "));
const spineNav = () => screen.findByRole("navigation", { name: SPINE });
const hrefIs = (href: string) => (link: HTMLElement) => link.getAttribute("href") === href;

/** The spine's row linking `href`: the last such link, after any the spine head draws. */
async function spineRow(href: string): Promise<HTMLElement> {
  const nav = await spineNav();
  return found(() => within(nav).queryAllByRole("link").filter(hrefIs(href)).at(-1), href);
}

async function pageLink(name: (name: string) => boolean, href: string): Promise<void> {
  const nav = await spineNav();
  const links = () => screen.queryAllByRole("link", { name }).filter(hrefIs(href));
  await found(() => links().find((link) => !nav.contains(link)), href);
}

/** Coverage row toggles (buttons with `aria-expanded`) and tiles (with `aria-pressed`). */
const withAttr = (attr: string, name: RegExp) =>
  screen.queryAllByRole("button", { name }).filter((button) => button.hasAttribute(attr));
const rowButtons = (name: RegExp) => withAttr("aria-expanded", name);
const tile = (name: RegExp) => withAttr("aria-pressed", name)[0]!;
const pressed = () =>
  [/All headings/, /Not covered/].map((name) => tile(name).getAttribute("aria-pressed"));

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

it("a changed document's spine row names its state", async () => {
  renderRoutes("/f/demo/product");
  // Section 02 (Beta) reads `revised` in the trace fixture.
  const links = await within(await spineNav()).findAllByRole("link", {
    name: (name) => /\bBeta\b/.test(name) && /changed since you settled it/i.test(name),
  });
  // The spine head says the same of Beta; the row is the other link.
  const [row] = links.filter((link) => !link.classList.contains("plan-spine-next"));
  expect(links).toHaveLength(2);
  expect(row!.querySelectorAll("[data-glyph]")).toHaveLength(1);
});

it("a running plan's spine row shows the activity roundel", async () => {
  checklist.plan_rows[0]!.state = "running";
  patchDoc(FIRST_PLAN, { progress: "running" });
  renderRoutes("/f/demo/engineering");
  const row = await spineRow("/f/demo/engineering/first");
  await waitFor(() => expect(row.querySelector(".aurora-roundel")).not.toBeNull());
  expect(row.querySelector(".activity-glyph__led")).toBeNull();
});

it("plan slugs drop the feature prefix in the spine", async () => {
  checklist.plan_rows[0]!.slug = "demo-trace";
  patchDoc(FIRST_PLAN, { slug: "demo-trace" });
  renderRoutes("/f/demo/engineering");
  const row = await spineRow("/f/demo/engineering/demo-trace");
  // What a sighted reader sees: the row's text without its `.sr-only` parts.
  const seen = row.cloneNode(true) as HTMLElement;
  seen.querySelectorAll(".sr-only").forEach((node) => node.remove());
  expect(norm(seen.textContent)).toMatch(/\btrace\b/);
  expect(norm(seen.textContent)).not.toContain("demo-trace");
  const titles = [row, ...row.querySelectorAll("[title]")].map((el) => el.getAttribute("title"));
  expect(titles).toContain("demo-trace");
});

it("the spine ends Product spec with Coverage and Plans with Findings, reading the trace", async () => {
  // The trace fixture: four headings, all covered; finding F-1 open, F-2 dismissed.
  renderRoutes("/f/demo/product");
  const spine = within(await spineNav());
  await spine.findByRole("link", { name: /\bAlpha\b/ });
  const expand = spine.queryByRole("button", { name: "Expand Plans" });
  if (expand) fireEvent.click(expand);
  await spine.findAllByRole("link", { name: /\bfirst\b/ });

  const coverage = await spine.findByRole("link", {
    name: "Coverage 4 of 4 headings covered",
    description: "every heading has a plan",
  });
  expect(coverage.getAttribute("href")).toBe("/f/demo/coverage");
  const coverageWell = coverage.closest("li")!;
  expect(coverageWell.getAttribute("data-instrument")).toBe("coverage");
  expect(coverageWell.parentElement?.lastElementChild).toBe(coverageWell);
  expect(coverageWell.querySelector("[data-tone]")).toBeNull();
  // One dot per heading, grouped by the two written sections.
  expect(coverageWell.querySelectorAll(".plan-spine__instrument-section")).toHaveLength(2);
  expect(coverageWell.querySelectorAll("[data-dot='covered']")).toHaveLength(4);

  const findings = await spine.findByRole("link", {
    name: "Findings 1 open",
    description: "from the first review and your reviews · 1 decided",
  });
  expect(findings.getAttribute("href")).toBe("/f/demo/findings");
  const findingsWell = findings.closest("li")!;
  expect(findingsWell.parentElement?.lastElementChild).toBe(findingsWell);
  expect(
    findingsWell.querySelector(".plan-spine__instrument-mark")?.getAttribute("data-tone"),
  ).toBe("warning");
});

it("the Findings row says nothing loud once every finding is decided", async () => {
  for (const { finding } of trace.findings) {
    finding.status = { kind: "dismissed", detail: "settled elsewhere" };
  }
  renderRoutes("/f/demo/engineering");
  const spine = within(await spineNav());
  await spine.findAllByRole("link", { name: /\bfirst\b/ });
  const findings = await spine.findByRole("link", { name: "Findings all clear" });
  expect(findings.closest("li")?.querySelector("[data-tone]")).toBeNull();
  expect(findings.closest("li")?.querySelectorAll("[data-dot='decided']")).toHaveLength(2);
});

it("the spine head names the top attention item", async () => {
  // The fixture frontier: section 02 (Beta) revised, then plan first's revisit, then triage.
  expect(trace.frontier.length).toBeGreaterThan(2);
  renderRoutes("/f/demo/product");
  const nav = await spineNav();
  const name = "Review Beta Changed since you settled it";
  const review = await within(nav).findAllByRole("link", { name });
  expect(review).toHaveLength(1);
  expect(review[0]!.getAttribute("href")).toBe("/f/demo/product/section_02");
  // The whole head is the one link: no primary button, and the reason sits inside it.
  const head = nav.querySelector<HTMLElement>(".plan-spine-next")!;
  expect(head).toBe(review[0]);
  expect(head.hasAttribute("data-look")).toBe(false);
  expect(head.classList.contains("plan-button")).toBe(false);
  expect(head.querySelector(".plan-spine-next__reason")?.textContent).toBe(
    "Changed since you settled it",
  );
  expect(nav.textContent).not.toMatch(/Revisit|findings from the last run|Triage|metaplan\b/);
  // The head links the top item alone; the Findings row below it is the spine's own.
  expect(within(head).queryAllByRole("link").filter(hrefIs("/f/demo/findings"))).toEqual([]);
});

it("a changed document offers See what changed", async () => {
  // Section 02 reads `revised` in the trace fixture.
  renderRoutes("/f/demo/product/section_02");
  await screen.findAllByRole("button", { name: /changed since you settled it/i });
  const named = (name: string) =>
    ["link", "button"].flatMap((role) => screen.queryAllByRole(role, { name }));
  await waitFor(() => expect(named("See what changed")).not.toEqual([]));
  expect(named("Changes since settled")).toEqual([]);
});

it("the scope shows as a standfirst linking its stub", async () => {
  renderRoutes("/f/demo/product/section_03");
  const settle = await screen.findByRole("button", { name: "Mark settled" });
  const headers = () => [...document.querySelectorAll("header")];
  const header = await found(() => headers().find((el) => el.querySelector("h1")), "the heading");
  // Section 03's stub is its row in the vision's scope table.
  const scope = await found(() => deepestHolding(header, SCOPE)[0], "the vision row");
  expect(scope.closest("[hidden], details:not([open])")).toBeNull();
  expect(follows(scope, settle)).toBe(true);
  const from = within(header).getByRole("link", { name: /from the vision/i });
  expect(from.getAttribute("href")).toMatch(
    /^\/f\/demo\/(vision|source\/product\/demo\/index\.md)/,
  );
});

it("a plan page lists what it implements and its briefs", async () => {
  const files = ["build/r1.md", "build/r2.md", "verify/v1.md"];
  patchDoc(FIRST_PLAN, { bundle: files.map((rel) => `doc/plans/demo/briefs/first/${rel}`) });
  renderRoutes("/f/demo/engineering/first");
  const name = "What it implements";
  const blocks = () => ["region", "list"].flatMap((role) => screen.queryAllByRole(role, { name }));
  const block = await found(() => blocks()[0], name);
  // Plan first covers heading One of section 01; Three and Four are plan second's.
  await waitFor(() => expect(block.textContent).toMatch(/\bOne\b/));
  expect(block.textContent).not.toMatch(/\b(Three|Four)\b/);
  const briefs = await screen.findByRole("list", { name: "Briefs" });
  for (const file of files) expect(briefs.textContent).toContain(file.split("/").pop());
});

it("an opened passage says what covers it", async () => {
  // Heading two loses its cover, so sentence s2 rests on an uncovered heading.
  trace.headings.find((h) => h.path === S01 && h.anchor === "two")!.covered_by = [];
  renderRoutes("/f/demo/product/section_01");
  await screen.findByRole("document", { name: "Reading column" });
  const planLinks = (root: HTMLElement) =>
    within(root).queryAllByRole("link").filter(hrefIs("/f/demo/engineering/first"));
  const opener = await screen.findByRole("button", { name: /^Intent is written down/ });
  expect(planLinks(opener.parentElement!)).toEqual([]);
  fireEvent.click(opener);
  let line = await holding("Covered by");
  while (line.parentElement && within(line).queryAllByRole("link").length === 0) {
    line = line.parentElement;
  }
  expect(planLinks(line)).not.toEqual([]);
  fireEvent.click(await screen.findByRole("button", { name: /^Each stage names/ }));
  await holding("Not covered by any plan");
});

it("the turn footer lists documents now out of step", async () => {
  // Plan first covers section 01's headings; the turn's edit put its settled spec out of step.
  patchDoc(FIRST_PLAN, { settle: "upstream_changed" });
  const changed_files = [{ path: S01, before_hash: "before", by_tool: true }];
  const fixture = chatSnapshotSchema.parse(snapshot).turns[0]!;
  const turn: Turn = { ...fixture, id: "t2", open: false, changed_files, summaries_rebuilding: [] };
  const footer = <TurnFooter turn={turn} sessionId="session one" />;
  renderWithProject(<MemoryRouter>{footer}</MemoryRouter>);
  const head = await holding("Now out of step with what you settled");
  const links = () => screen.queryAllByRole("link", { name: /\bFirst\b/ });
  await found(() => links().find((link) => follows(head, link)), "plan First below the heading");
});

it("coverage tiles filter the heading rows", async () => {
  // Section 03 gains headings Five and Six, which nothing covers.
  for (const title of ["Five", "Six"]) {
    const anchor = title.toLowerCase();
    trace.headings.push({ path: S03, anchor, title, covered_by: [], findings: [] });
    trace.gaps.push({ kind: "uncovered", node: { path: S03, anchor }, count: null });
  }
  renderRoutes("/f/demo/coverage");
  await waitFor(() => expect(rowButtons(HEADING_NAME).length).toBeGreaterThanOrEqual(6));
  expect(pressed()).toEqual(["true", "false"]);
  fireEvent.click(tile(/Not covered/));
  await waitFor(() => {
    expect(rowButtons(HEADING_NAME)).toHaveLength(2);
    expect(rowButtons(UNCOVERED)).toHaveLength(2);
  });
  expect(pressed()).toEqual(["false", "true"]);
});

it("a coverage row expands to why, how and findings", async () => {
  renderRoutes("/f/demo/coverage");
  const shownWhy = () => screen.queryAllByText(/^Why$/).filter((el) => !el.closest("[hidden]"));
  const opened = (el: Element) =>
    /\bWhy\b/.test(el.textContent!) && /\bHow\b/.test(el.textContent!);
  // Heading One of section 01 is covered by plan first.
  const row = await found(() => rowButtons(/\bOne\b/)[0], "the row of heading One");
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(shownWhy()).toEqual([]);
  fireEvent.click(row);
  await waitFor(() => expect(row.getAttribute("aria-expanded")).toBe("true"));
  const controls = row.getAttribute("aria-controls");
  let open: HTMLElement | null = controls ? document.getElementById(controls) : row;
  while (open && !opened(open)) open = open.parentElement;
  if (!open) throw new Error("the row opened no Why and How");
  expect(open.closest("[hidden]")).toBeNull();
  expect(open.textContent).toMatch(/\bFindings\b/);
  expect(open.textContent).toMatch(/\bfirst\b/);
});

it("findings group by run and fold decided ones", async () => {
  // F-1 is a run finding and F-2 a dismissed one; F-3 comes from Review the feature.
  const [run] = trace.findings;
  const source = "review 2026-11-06";
  trace.findings.push({ finding: { ...run!.finding, id: "F-3", source }, about: run!.about });
  renderRoutes("/f/demo/findings");
  const [runHead] = await screen.findAllByRole("heading", { name: /^Run of/ });
  const reviews = await screen.findByRole("heading", { name: "Reviews" });
  expect(follows(runHead!, reviews)).toBe(true);
  const line = await holding("Dismissed:", "not needed");
  expect(norm(line.textContent)).toMatch(/Dismissed:\s*not needed/);
  const [folded] = deepestHolding(document.body, "Rename the flag", "Dismissed:");
  expect(within(folded!).queryAllByRole("button", { name: /^Dismiss\b/ })).toEqual([]);
});

it("the product overview draws no Coverage link of its own; the Run page links Findings", async () => {
  renderRoutes("/f/demo/product");
  const nav = await spineNav();
  await screen.findAllByRole("link", { name: /^Coverage\b/ });
  // The spine's row is the one way to Coverage from the overview.
  const outside = screen
    .queryAllByRole("link")
    .filter(hrefIs("/f/demo/coverage"))
    .filter((link) => !nav.contains(link));
  expect(outside).toEqual([]);
  cleanup();
  // Finding F-1 is open in the trace fixture.
  renderRoutes("/f/demo/run");
  await pageLink((name) => /Findings/.test(name) && /\b\d+ open\b/.test(name), "/f/demo/findings");
});
