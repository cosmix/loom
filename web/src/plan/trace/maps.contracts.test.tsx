import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
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
import { traceViewSchema, type CoverRef, type TraceView } from "@/plan/api/schema-trace";
import { runOffer } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));
// Offers are observed, never run: no session starts in these contracts.
vi.mock("@/plan/offers/actions", async (original) => ({
  ...(await original<typeof import("@/plan/offers/actions")>()),
  runOffer: vi.fn(() => true),
}));

const SPINE = "Feature documents";
const WIDE_QUERY = "(min-width: 1024px)";
const S03 = "doc/product/demo/section_03.md";
const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const THIRD_PLAN = "doc/plans/demo/PLAN-third.md";
/** The ledger's save hash, as the file view (`/api/doc`) serves it. */
const HASH = "sha256:1ed9e7c0ffee";
const HEADING_NAME = /\b(One|Two|Three|Four|Five|Six|Seven)\b/;
const UNCOVERED = /\b(Five|Six)\b/;
const MARKUP_TITLE = "<img src=x onerror=alert(1)>";
const MARKUP_TEXT = "**bold**";

type Call = { url: URL; method: string; body: string | null };

let calls: Call[] = [];
let trace: TraceView;

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

/** The fixture trace, with section 03 written so its headings are drawn like any other. */
function baseTrace(): TraceView {
  const view = traceViewSchema.parse(structuredClone(traceFixture));
  view.documents.find((doc) => doc.path === S03)!.progress = "written";
  return view;
}

function heading(anchor: string, title: string, covered: CoverRef[] = []) {
  return { path: S03, anchor, title, covered_by: covered, findings: [] };
}

/** Section 03 gains headings Five and Six, which nothing covers. */
function addUncovered(view: TraceView): void {
  view.headings.push(heading("five", "Five"), heading("six", "Six"));
  for (const anchor of ["five", "six"]) {
    view.gaps.push({ kind: "uncovered", node: { path: S03, anchor }, count: null });
  }
}

/** Section 03 gains heading Seven, covered by plan third alone: no entry, no stage, no finding. */
function addPlanOnlyHeading(view: TraceView): void {
  const first = view.documents.find((doc) => doc.path === FIRST_PLAN)!;
  view.documents.push({ ...first, path: THIRD_PLAN, title: "Third", slug: "third", stub: null });
  // Heading One's second cover is plan first itself, with no stage.
  const cover = { ...view.headings[0]!.covered_by[1]!, path: THIRD_PLAN, slug: "third" };
  view.headings.push(heading("seven", "Seven", [cover]));
}

function answer(url: URL, method: string): Response {
  const { pathname } = url;
  if (pathname === "/api/tree") return json(treeFixture);
  if (pathname === "/api/trace/demo") return json(trace);
  if (pathname === "/api/doc") {
    return json({ ...docFixture, path: `doc/${url.searchParams.get("path")}`, hash: HASH });
  }
  if (pathname === "/api/findings/status" && method === "POST") {
    return json({ result: "saved", hash: "sha256:after", validation: null });
  }
  if (pathname === "/api/implemented-by") return json({ sections: {} });
  return json({ error: "not_found", message: "no such page" }, 404);
}

function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = requestUrl(input);
    const method = init?.method ?? (input instanceof Request ? input.method : "GET");
    const body = typeof init?.body === "string" ? init.body : null;
    calls.push({ url, method, body });
    return Promise.resolve(answer(url, method));
  });
}

function statusPosts(): unknown[] {
  return calls
    .filter((call) => call.method === "POST" && call.url.pathname === "/api/findings/status")
    .map((call) => (call.body === null ? null : JSON.parse(call.body)));
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

function offer(kind: string, label: string) {
  return { id: kind, kind, label, target: null, feature: "demo" };
}

/** The demo feature with three written sections, plan first, and its follow-up offers. */
function demoChecklist(): ChecklistView {
  const base = checklistFixture.checklists[0]!;
  return checklistViewSchema.parse({
    ...base,
    sections: [section("01", "Alpha"), section("02", "Beta"), section("03", "Gamma")],
    plan_rows: [{ ...base.plan_rows[0]!, slug: "first", path: FIRST_PLAN, title: "First" }],
    secondary: [
      offer("plan_follow_ups", "Plan follow-ups"),
      offer("review_feature", "Review the feature"),
    ],
  });
}

function renderRoutes(path: string): void {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [demoChecklist()], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

/** The accessible description: the described-by text, an `aria-description`, or a title. */
function description(element: Element): string {
  const ids = (element.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
  const described = ids.map((id) => document.getElementById(id)?.textContent ?? "");
  const own = [element.getAttribute("aria-description"), element.getAttribute("title")];
  return [...described, ...own].filter(Boolean).join(" ");
}

function headingCards(): HTMLElement[] {
  return screen.queryAllByRole("button", { name: HEADING_NAME });
}

function card(name: RegExp): HTMLElement {
  const [found, ...rest] = screen.getAllByRole("button", { name });
  if (!found || rest.length > 0) throw new Error(`expected one card named ${name}`);
  return found;
}

/** The filter control named `name`, whatever form the page draws it as. */
function filterControl(name: string): HTMLElement {
  for (const role of ["radio", "tab", "button", "checkbox", "menuitemradio", "option"] as const) {
    const [found] = screen.queryAllByRole(role, { name });
    if (found) return found;
  }
  throw new Error(`no filter named ${name}`);
}

/** The focus panel: the page's `aside` other than the chat region. */
function focusPanel(): HTMLElement {
  const asides = screen.queryAllByRole("complementary");
  const panels = asides.filter((aside) => aside.getAttribute("aria-label") !== "Chat");
  if (panels.length !== 1) throw new Error(`expected one focus panel, saw ${panels.length}`);
  return panels[0]!;
}

async function pressAndExpectFocus(key: string, name: RegExp): Promise<HTMLElement> {
  fireEvent.keyDown(document.activeElement ?? document.body, { key });
  return waitFor(() => {
    const active = document.activeElement as HTMLElement;
    expect(active.getAttribute("role") ?? active.tagName.toLowerCase()).toBe("button");
    expect(active.textContent + " " + (active.getAttribute("aria-label") ?? "")).toMatch(name);
    return active;
  });
}

function expectPanelNames(name: RegExp): void {
  const panel = focusPanel();
  expect(within(panel).getAllByRole("heading")[0]!.textContent).toMatch(name);
}

/** The finding card holding `title`: the nearest ancestor of the title holding `action`. */
function findingCard(title: string, action: RegExp): HTMLElement {
  const holds = (element: Element) => (element.textContent ?? "").includes(title);
  const deepest = [...document.body.querySelectorAll<HTMLElement>("*")].find(
    (element) => holds(element) && ![...element.children].some(holds),
  );
  let node: HTMLElement | null = deepest ?? null;
  while (node && within(node).queryAllByRole("button", { name: action }).length === 0) {
    node = node.parentElement;
  }
  if (!node) throw new Error(`no card holds ${title} with ${action}`);
  return node;
}

/** Two more open findings, so the open count (three) differs from the total (four). */
function addOpenFindings(view: TraceView): void {
  const [first] = view.findings;
  for (const id of ["F-4", "F-5"]) {
    view.findings.push({
      finding: { ...first!.finding, id, title: `Open finding ${id}` },
      about: first!.about,
    });
  }
  const triage = view.gaps.find((gap) => gap.kind === "triage");
  if (triage) triage.count = 3;
}

beforeEach(() => {
  window.localStorage.clear();
  calls = [];
  trace = baseTrace();
  vi.mocked(runOffer).mockClear();
  stubMedia();
  stubServer();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("the coverage page marks uncovered headings and filters to them", async () => {
  addUncovered(trace);
  renderRoutes("/f/demo/coverage");

  await waitFor(() => expect(headingCards().length).toBeGreaterThanOrEqual(6));
  for (const cardElement of headingCards()) {
    const uncovered =
      UNCOVERED.test(cardElement.textContent ?? "") ||
      UNCOVERED.test(cardElement.getAttribute("aria-label") ?? "");
    const said = description(cardElement);
    if (uncovered) expect(said, cardElement.outerHTML).toMatch(/\bNot covered\b/);
    else expect(said, cardElement.outerHTML).not.toMatch(/\bNot covered\b/);
  }

  fireEvent.click(filterControl("Uncovered"));

  await waitFor(() => {
    const names = headingCards().map((element) => element.textContent ?? "");
    expect(names).toHaveLength(2);
    expect(
      names.every((name) => UNCOVERED.test(name)),
      `${names}`,
    ).toBe(true);
  });
});

it("focus keys walk why and how", async () => {
  addPlanOnlyHeading(trace);
  renderRoutes("/f/demo/coverage");

  const seven = await waitFor(() => card(/\bSeven\b/));
  fireEvent.click(seven);
  seven.focus();
  await waitFor(() => expectPanelNames(/\bSeven\b/));

  const plan = await pressAndExpectFocus("l", /\bthird\b/i);
  expect(plan).not.toBe(card(/\bSeven\b/));
  await waitFor(() => expectPanelNames(/\bthird\b/i));

  const back = await pressAndExpectFocus("h", /\bSeven\b/);
  expect(back).toBe(card(/\bSeven\b/));
  await waitFor(() => expectPanelNames(/\bSeven\b/));

  const vision = await pressAndExpectFocus("h", /\bGamma\b|\b0?3\b/);
  expect(vision).not.toBe(card(/\bSeven\b/));
  await waitFor(() => expectPanelNames(/\bGamma\b|\b0?3\b/));
});

it("dismiss needs a reason and posts the status", async () => {
  renderRoutes("/f/demo/findings");

  const findingOne = await waitFor(() => findingCard("Rebuild ignores briefs", /^Dismiss\b/));
  fireEvent.click(within(findingOne).getByRole("button", { name: /^Dismiss\b/ }));

  const reason = await screen.findByRole("textbox", { name: /reason/i });
  let holder: HTMLElement | null = reason.parentElement;
  const confirmName = /dismiss|confirm/i;
  while (holder && within(holder).queryAllByRole("button", { name: confirmName }).length === 0) {
    holder = holder.parentElement;
  }
  if (!holder) throw new Error("the reason field has no confirm button beside it");
  const confirm = () => within(holder!).getAllByRole("button", { name: confirmName }).at(-1)!;
  const blocked = (button: HTMLElement) =>
    (button as HTMLButtonElement).disabled || button.getAttribute("aria-disabled") === "true";

  expect(blocked(confirm())).toBe(true);
  fireEvent.click(confirm());
  expect(statusPosts()).toEqual([]);

  fireEvent.change(reason, { target: { value: "duplicate" } });
  await waitFor(() => expect(blocked(confirm())).toBe(false));
  fireEvent.click(confirm());

  await waitFor(() => expect(statusPosts()).toHaveLength(1));
  expect(statusPosts()[0]).toEqual({
    feature: "demo",
    id: "F-1",
    hash: HASH,
    status: { kind: "dismissed", detail: "duplicate" },
  });
});

it("Plan this seeds Plan follow-ups with the finding", async () => {
  renderRoutes("/f/demo/findings");

  const findingOne = await waitFor(() => findingCard("Rebuild ignores briefs", /^Plan this\b/));
  fireEvent.click(within(findingOne).getByRole("button", { name: /^Plan this\b/ }));

  await waitFor(() => expect(vi.mocked(runOffer)).toHaveBeenCalledTimes(1));
  const [sent, seed] = vi.mocked(runOffer).mock.calls[0]!;
  expect(sent).toMatchObject({ kind: "plan_follow_ups", feature: "demo" });
  expect(seed).toEqual(expect.stringContaining("F-1"));
  expect(seed).toEqual(expect.stringContaining("Rebuild ignores briefs"));
});

it("finding markup renders as text", async () => {
  const [first] = trace.findings;
  trace.findings.push({
    finding: { ...first!.finding, id: "F-3", title: MARKUP_TITLE, text: MARKUP_TEXT },
    about: first!.about,
  });
  renderRoutes("/f/demo/findings");

  await waitFor(() => expect(document.body.textContent).toContain(MARKUP_TITLE));
  expect(document.body.textContent).toContain(MARKUP_TEXT);
  expect(document.querySelectorAll('img[src="x"]')).toHaveLength(0);
  const bold = [...document.querySelectorAll("strong, b")].filter((element) =>
    /^\s*bold\s*$/.test(element.textContent ?? ""),
  );
  expect(bold).toEqual([]);
});

it("the spine links Coverage and Findings with the open count", async () => {
  addOpenFindings(trace);
  renderRoutes("/f/demo/product");
  const spine = within(await screen.findByRole("navigation", { name: SPINE }));

  const coverage = await spine.findByRole("link", { name: /^Coverage\b/ });
  expect(coverage.getAttribute("href")).toBe("/f/demo/coverage");

  const expand = spine.queryByRole("button", { name: "Expand Run" });
  if (expand) fireEvent.click(expand);

  // Three open findings (F-1, F-4, F-5) of four; F-2 is dismissed.
  const isFindings = (name: string) => /\bFindings\b/.test(name);
  const findings = await waitFor(() => {
    const rows = spine
      .getAllByRole("link", { name: isFindings })
      .filter((link) => link.getAttribute("href") === "/f/demo/findings");
    expect(rows).toHaveLength(1);
    return rows[0]!;
  });
  const counted = spine.queryAllByRole("link", {
    name: (name) => isFindings(name) && /\b3\b/.test(name),
  });
  expect(counted).toContain(findings);
});
