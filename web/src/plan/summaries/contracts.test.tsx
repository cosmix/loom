import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ComponentType, ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import type { connectPlanSocket } from "@/plan/api/socket";
import {
  authoringAtom,
  connectionEpochAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

type Obj = Record<string, unknown>;
type Store = ReturnType<typeof createStore>;
type Answer = (rebuild: boolean) => void;
type QuestionProps = { question: Obj; canAnswer: boolean; onAnswer: Answer };
type Who = "holder" | "other" | "read_only";

const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const DANA = { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" };
const TMUX = [{ what: "tmux", message: "tmux is not installed", fix: "Install tmux" }];
const NOW_SECS = 1793261460;
const PANEL_PATH = "/api/summaries/panel";

let replies: Array<{ status: number; body: unknown }> = [];
let fetchMock = vi.fn<typeof fetch>();
let handlers = new Map<string, Set<(message: Obj) => void>>();
let resyncs = new Set<() => void>();

// The components do not exist before the implementation: a variable specifier keeps vite from
// resolving them at collection, so each missing module fails only the contracts that load it.
async function load<T>(name: string): Promise<T> {
  const specifier = `./${name}`;
  return (await import(/* @vite-ignore */ specifier)) as T;
}

function pathOf(input: RequestInfo | URL): string {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost").pathname;
}

// GET /api/summaries/panel answers from `replies` in order, repeating the last; anything else 404s.
async function answer(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const isPanel = pathOf(input) === PANEL_PATH && (init?.method ?? "GET") === "GET";
  const reply = isPanel ? (replies.length > 1 ? replies.shift() : replies[0]) : undefined;
  const { status, body } = reply ?? { status: 404, body: { error: "not_found" } };
  const headers = { "content-type": "application/json" };
  return new Response(JSON.stringify(body), { status, headers });
}

const calls = (path: string, method: string) =>
  fetchMock.mock.calls.filter(([input, init]) => {
    return pathOf(input) === path && (init?.method ?? "GET") === method;
  });

// Plan 2's socket handle (`planSocketAtom`) with frames delivered straight to its subscribers.
function fakeSocket(): ReturnType<typeof connectPlanSocket> {
  const subscribe = (type: string, handler: (message: Obj) => void) => {
    const set = handlers.get(type) ?? new Set();
    handlers.set(type, set.add(handler));
    return () => void set.delete(handler);
  };
  const onResync = (handler: () => void) => {
    resyncs.add(handler);
    return () => void resyncs.delete(handler);
  };
  const handle = { send: () => true, subscribe, onResync, dispose: () => undefined };
  return handle as unknown as ReturnType<typeof connectPlanSocket>;
}

const deliver = (type: string, body: Obj) =>
  act(() => handlers.get(type)?.forEach((handler) => handler({ type, ...body })));

// What plan 2's socket does on a `hello` after the page's first: bump the epoch, then resync.
function reconnect(store: Store): void {
  act(() => {
    store.set(connectionEpochAtom, (epoch) => epoch + 1);
    resyncs.forEach((handler) => handler());
  });
}

function storeFor(who: Who): Store {
  const store = createStore();
  initClient(store);
  const server = { ...helloFixture.server, read_only: who === "read_only" ? TMUX : [], you: ADA };
  const holder = who === "read_only" ? null : who === "holder" ? ADA : DANA;
  store.set(youAtom, ADA);
  store.set(serverInfoAtom, server as never);
  store.set(authoringAtom, { holder: holder && { ...holder, away_since: null, frees_at: null } });
  store.set(planSocketAtom, fakeSocket());
  return store;
}

function show(ui: ReactNode, store: Store = storeFor("holder")) {
  return render(
    <Provider store={store}>
      <TooltipProvider>{ui}</TooltipProvider>
    </Provider>,
  );
}

// Text with a space where text nodes of different elements meet, so adjacent figures stay apart.
function text(element: Element): string {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let joined = "";
  let parent: Node | null = null;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    joined += (parent !== null && node.parentNode !== parent ? " " : "") + node.nodeValue;
    parent = node.parentNode;
  }
  return joined.replace(/\s+/g, " ");
}
const blocked = (element: HTMLElement): boolean =>
  element.hasAttribute("disabled") || element.getAttribute("aria-disabled") === "true";

// A token figure as the dashboard may print it: 1,840, 1840 or 1.8k.
function figure(value: number): string {
  const forms = [value.toLocaleString("en-US"), String(value)];
  if (value >= 1000) forms.push(`${Number((value / 1000).toFixed(1))}k`);
  const escaped = forms.map((form) => form.replace(/[.,]/g, (mark) => `\\${mark}`));
  return `(?<![\\d.,])(?:${escaped.join("|")})(?![\\d,]|\\.\\d)`;
}

// The element with the shortest text that matches every pattern.
function smallestWith(root: Element, ...patterns: RegExp[]): Element {
  const found = Array.from(root.querySelectorAll("*"))
    .filter((element) => patterns.every((pattern) => pattern.test(text(element))))
    .sort((a, b) => text(a).length - text(b).length)[0];
  if (!found) throw new Error(`nothing matches ${patterns.join(" and ")}`);
  return found;
}

// Each label sits beside its own figure, whether labels lead or follow their figures.
function labelled(line: string, pairs: Array<[string, number]>): boolean {
  const gap = "[^0-9]{0,40}";
  const leads = pairs.every(([label, value]) =>
    new RegExp(label + gap + figure(value), "i").test(line),
  );
  const follows = pairs.every(([label, value]) =>
    new RegExp(figure(value) + gap + label, "i").test(line),
  );
  return leads || follows;
}

function panelWith(fields: Obj): Obj {
  return { ...structuredClone(panelFixture), ...fields };
}

// The last rebuild's cost line: the smallest element holding its fresh and cache-read figures.
const costLine = (root: Element): string =>
  text(smallestWith(root, new RegExp(figure(1840)), new RegExp(figure(9200))));

const question = (): Obj => structuredClone(questionFixture);

beforeEach(() => {
  replies = [{ status: 200, body: panelWith({}) }];
  handlers = new Map();
  resyncs = new Set();
  fetchMock = vi.fn<typeof fetch>(answer);
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("panel shows running rebuild, last cost, totals and quota", async () => {
  type Props = { panel: Obj; nowSecs: number };
  const { SummaryPanel } = await load<{ SummaryPanel: ComponentType<Props> }>("summary-panel");
  const { container } = show(<SummaryPanel panel={panelWith({})} nowSecs={NOW_SECS} />);
  const all = text(container);
  expect(all).toMatch(/Rebuilding \d of 3/);
  for (const id of ["product/section_01", "product/section_02", "product/section_03"]) {
    expect(all).toContain(id);
  }
  expect(all).toContain("no seal in 10 minutes");

  const line = costLine(container);
  const pairs: Array<[string, number]> = [
    ["fresh", 1840],
    ["cache\\w* ?reads?", 9200],
    ["output", 760],
  ];
  expect(labelled(line, pairs), `fresh, cache reads and output in "${line}"`).toBe(true);
  expect(all).toMatch(/today/i);
  expect(all).toMatch(/7.day|week/i);
  expect(all).toMatch(new RegExp(figure(12400)));
  expect(all).toMatch(new RegExp(figure(74600)));
  expect(screen.getByRole("img", { name: /^claude 5h window 48% used/ })).toBeTruthy();
  expect(screen.getByRole("img", { name: /^claude 7d window 31% used/ })).toBeTruthy();
  expect(screen.getByRole("img", { name: /^codex 7d window 63% used/ })).toBeTruthy();
  cleanup();

  const last = { ...panelFixture.last, tokens: { fresh_input: 1840, cache_read: 9200 } };
  const partial = show(<SummaryPanel panel={panelWith({ last })} nowSecs={NOW_SECS} />);
  const missing = costLine(partial.container);
  expect(missing, "the absent output figure").toMatch(/unknown/i);
  expect(missing, "an absent field printed as 0").not.toMatch(/(?<![\d.,])0(?![\d.,])/);
});

it("question offers rebuild only to a browser that may answer", async () => {
  type Module = { RebuildQuestion: ComponentType<QuestionProps> };
  const { RebuildQuestion } = await load<Module>("rebuild-question");
  const refused = vi.fn<Answer>();
  const first = show(
    <RebuildQuestion question={question()} canAnswer={false} onAnswer={refused} />,
    storeFor("other"),
  );
  expect(text(first.container)).toMatch(/(?<!\d)6(?!\d)/);
  for (const name of [/rebuild all/i, /not now/i]) {
    const button = screen.getByRole("button", { name });
    expect(blocked(button), `${String(name)} is disabled for a read-only browser`).toBe(true);
    fireEvent.click(button);
  }
  expect(refused).not.toHaveBeenCalled();
  cleanup();

  const allowed = vi.fn<Answer>();
  show(<RebuildQuestion question={question()} canAnswer onAnswer={allowed} />);
  const notNow = screen.getByRole("button", { name: /not now/i });
  expect(blocked(notNow)).toBe(false);
  fireEvent.click(notNow);
  expect(allowed.mock.calls).toEqual([[false]]);
});

it("rebuilt summary rows show tokens or tokens unknown", async () => {
  type Module = { SummaryTokens: ComponentType<{ tokens: number | null }> };
  const { SummaryTokens } = await load<Module>("summary-tokens");
  const known = show(<SummaryTokens tokens={12000} />);
  expect(text(known.container)).toContain("12,000 tokens");
  cleanup();

  const unknown = show(<SummaryTokens tokens={null} />);
  const shown = text(unknown.container);
  expect(shown).toContain("tokens unknown");
  expect(shown).not.toMatch(/(?<![\d.,])0(?![\d.,])|null/);
});

it("panel shows the read-only reason instead of controls", async () => {
  type Props = { panel: Obj; nowSecs: number };
  const { SummaryPanel } = await load<{ SummaryPanel: ComponentType<Props> }>("summary-panel");
  const { RebuildButton } = await load<{ RebuildButton: ComponentType<{ ids: string[] }> }>(
    "rebuild-button",
  );
  const panel = panelWith({ read_only: "tmux is not installed" });
  // A writable browser: the reason and the missing controls come from the panel's own field.
  const shown = show(<SummaryPanel panel={panel} nowSecs={NOW_SECS} />, storeFor("holder"));
  expect(text(shown.container)).toContain("tmux is not installed");
  const live = screen.queryAllByRole("button", { name: /rebuild/i }).filter((b) => !blocked(b));
  expect(live, "an enabled rebuild control on a read-only panel").toEqual([]);
  cleanup();

  show(<RebuildButton ids={["product"]} />, storeFor("read_only"));
  for (const button of screen.queryAllByRole("button", { name: /rebuild/i })) {
    expect(blocked(button), "a rebuild button a read-only browser can press").toBe(true);
    fireEvent.click(button);
  }
  expect(calls("/api/summaries/rebuild", "POST")).toEqual([]);
  cleanup();

  show(<RebuildButton ids={["product"]} />, storeFor("holder"));
  fireEvent.click(screen.getByRole("button", { name: /rebuild/i }));
  await waitFor(() => expect(calls("/api/summaries/rebuild", "POST")).toHaveLength(1));
  const [, init] = calls("/api/summaries/rebuild", "POST")[0];
  expect(JSON.parse(String(init?.body))).toEqual({ ids: ["product"] });
});

it("approval notice shows the stale count and an approve button only for the author", async () => {
  type Module = { ApprovalNotice: ComponentType<QuestionProps> };
  const { ApprovalNotice } = await load<Module>("approval-notice");
  const approve = /rebuild all|approve/i;
  const author = vi.fn<Answer>();
  const mine = show(<ApprovalNotice question={question()} canAnswer onAnswer={author} />);
  expect(text(mine.container)).toMatch(/(?<!\d)6(?!\d)/);
  const button = screen.getByRole("button", { name: approve });
  expect(blocked(button)).toBe(false);
  fireEvent.click(button);
  expect(author.mock.calls).toEqual([[true]]);
  cleanup();

  const reader = vi.fn<Answer>();
  const theirs = show(
    <ApprovalNotice question={question()} canAnswer={false} onAnswer={reader} />,
    storeFor("other"),
  );
  expect(text(theirs.container)).toMatch(/(?<!\d)6(?!\d)/);
  for (const offered of screen.queryAllByRole("button", { name: approve })) {
    expect(blocked(offered), "an approve button a read-only browser can press").toBe(true);
    fireEvent.click(offered);
  }
  expect(reader).not.toHaveBeenCalled();
});

type StatusModule = { SummaryStatus: ComponentType<{ compact?: boolean }> };
const loadStatus = () => load<StatusModule>("summary-status");

it("an arriving question opens the popover without taking focus", async () => {
  const { SummaryStatus } = await loadStatus();
  show(
    <>
      <textarea aria-label="Message" />
      <SummaryStatus />
    </>,
  );
  await waitFor(() => expect(calls(PANEL_PATH, "GET").length).toBeGreaterThan(0));
  const textarea = screen.getByRole("textbox", { name: "Message" });
  act(() => textarea.focus());
  deliver("summary_question", question());

  await screen.findByRole("button", { name: /not now/i });
  expect(screen.getAllByRole("button", { name: /rebuild all/i }).length).toBeGreaterThan(0);
  expect(document.activeElement, "focus left the textarea").toBe(textarea);
  const regions = Array.from(document.querySelectorAll('[aria-live="polite"], [role="status"]'));
  const announced = regions.filter((region) => /(?<!\d)6(?!\d)/.test(text(region)));
  expect(announced.length, "the question in a polite live region").toBeGreaterThan(0);

  const target = document.activeElement ?? document.body;
  fireEvent.keyDown(target, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(target, { key: "Enter", code: "Enter" });
  await act(async () => await Promise.resolve());
  expect(calls("/api/summaries/answer", "POST")).toEqual([]);
});

it("the panel reseeds after a reconnect", async () => {
  const { SummaryStatus } = await loadStatus();
  const idle = { running: null, queue: [], failed: [], question: null };
  replies = [
    { status: 200, body: panelWith({}) },
    { status: 200, body: panelWith(idle) },
  ];
  const store = storeFor("holder");
  show(<SummaryStatus />, store);
  expect((await screen.findAllByText(/Rebuilding/)).length).toBeGreaterThan(0);
  deliver("summary_question", question());
  await screen.findByRole("button", { name: /not now/i });

  reconnect(store);
  await waitFor(() => expect(calls(PANEL_PATH, "GET")).toHaveLength(2));
  await waitFor(() => {
    expect(screen.queryAllByText(/Rebuilding/), "a finished rebuild kept").toEqual([]);
    expect(screen.queryAllByRole("button", { name: /not now/i }), "a closed question kept").toEqual(
      [],
    );
  });
});

it("a failed panel fetch shows its error", async () => {
  const { SummaryStatus } = await loadStatus();
  replies = [
    { status: 500, body: { error: "internal", message: "panel failed" } },
    { status: 200, body: panelWith({}) },
  ];
  show(<SummaryStatus />);
  await waitFor(() => expect(calls(PANEL_PATH, "GET")).toHaveLength(1));
  // The status control is the only button rendered before its popover opens.
  fireEvent.click((await screen.findAllByRole("button"))[0]);

  const retry = await screen.findByRole("button", { name: /retry/i });
  expect(screen.queryByRole("status", { name: /loading/i }), "stuck on the skeleton").toBe(null);
  fireEvent.click(retry);
  await waitFor(() => expect(calls(PANEL_PATH, "GET")).toHaveLength(2));
  expect((await screen.findAllByText(/Rebuilding/)).length).toBeGreaterThan(0);
});

it("the compact header control shows a glyph and count", async () => {
  const { SummaryStatus } = await loadStatus();
  show(<SummaryStatus compact />);
  const control = await screen.findByRole("button", { name: /rebuild/i });
  await waitFor(() => expect(control.textContent).toMatch(/(?<!\d)3(?!\d)/));

  const visible = control.cloneNode(true) as HTMLElement;
  visible.querySelectorAll(".sr-only").forEach((hidden) => hidden.remove());
  expect(text(visible), "the full text label in the compact control").not.toMatch(/rebuilding/i);
  expect(text(visible)).toMatch(/(?<!\d)3(?!\d)/);
  expect(control.querySelector("svg, [aria-hidden='true']"), "the glyph").not.toBe(null);
});
