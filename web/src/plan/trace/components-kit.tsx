import { render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { traceViewSchema, type TraceDoc, type TraceView } from "@/plan/api/schema-trace";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { traceAtom } from "./state";

// The shared harness of the trace components' tests: the trace fixture, a fetch stub that answers
// the routes they use, and a mount that holds the trace as the feature layout's spine would.

export const VISION = "doc/product/demo/index.md";
export const S01 = "doc/product/demo/section_01.md";
export const S02 = "doc/product/demo/section_02.md";
export const S03 = "doc/product/demo/section_03.md";
export const METAPLAN = "doc/plans/demo/index.md";
export const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
export const HASH = "sha256:5e771ed0c0ffee";
export const COMMIT = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";
export const CURRENT_TEXT = "current text";
export const VISION_TEXT = [
  "# Demo vision",
  "",
  "| #   | Section | Covers                |",
  "| --- | ------- | --------------------- |",
  "| 02  | Beta    | What beta promises    |",
].join("\n");
const METAPLAN_TEXT = [
  "# Metaplan",
  "",
  "## 1. First (`PLAN-first.md`)",
  "",
  "Covers: section_01#one",
  "",
  "Build the first plan",
  "from one idea.",
  "",
  "## 2. Second (`PLAN-second.md`)",
].join("\n");
const METAPLAN_SECTIONS = [
  {
    level: 2,
    title: "1. First (`PLAN-first.md`)",
    anchor: "1-first-plan-firstmd",
    start_line: 3,
    end_line: 9,
  },
  {
    level: 2,
    title: "2. Second (`PLAN-second.md`)",
    anchor: "2-second-plan-secondmd",
    start_line: 10,
    end_line: 10,
  },
];

export type Call = { url: URL; method: string };
export type Answer = { status: number; body: unknown };
type Held = { text: string; sections: unknown[] };

/** The mutable state of one test; `resetKit` makes it fresh. */
export const kit = {
  calls: [] as Call[],
  trace: undefined as unknown as TraceView,
  settleAnswers: [] as Answer[],
  settledCommit: COMMIT as string | null,
  /** Documents whose text a test sets, by path relative to `doc/`. */
  docs: {} as Record<string, Held>,
};

export function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function docAnswer(rel: string): unknown {
  const held = kit.docs[rel];
  const text =
    held?.text ??
    (rel === "product/demo/index.md"
      ? VISION_TEXT
      : rel === "plans/demo/index.md"
        ? METAPLAN_TEXT
        : CURRENT_TEXT);
  const sections = held?.sections ?? (rel === "plans/demo/index.md" ? METAPLAN_SECTIONS : []);
  return { ...docFixture, path: `doc/${rel}`, hash: HASH, text, sections };
}

function answer(url: URL, method: string): Response {
  const { pathname, searchParams } = url;
  if (pathname === "/api/trace/demo") return json(kit.trace);
  if (pathname === "/api/settle" && method === "POST") {
    const next = kit.settleAnswers.shift();
    return next
      ? json(next.body, next.status)
      : json({ result: "saved", hash: "sha256:after", validation: null });
  }
  if (pathname === "/api/settled-version") return json({ commit: kit.settledCommit });
  if (pathname === "/api/at-commit") {
    return json({ path: searchParams.get("path"), text: "settled text" });
  }
  if (pathname === "/api/doc") return json(docAnswer(searchParams.get("path") ?? ""));
  return json({ error: "not_found", message: "no such route" }, 404);
}

export function patchDoc(path: string, patch: Partial<TraceDoc>): void {
  const found = kit.trace.documents.find((doc) => doc.path === path);
  if (!found) throw new Error(`the trace fixture holds no ${path}`);
  Object.assign(found, patch);
}

/** A fresh trace and fetch stub; call from `beforeEach`. */
export function resetKit(): void {
  kit.calls = [];
  kit.settleAnswers = [];
  kit.settledCommit = COMMIT;
  kit.docs = {};
  kit.trace = traceViewSchema.parse(structuredClone(traceFixture));
  // The control tests settle section 03 as a written draft; the fixture lists it unwritten.
  patchDoc(S03, { progress: "written", settleable: true });
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    const method = init?.method ?? "GET";
    kit.calls.push({ url, method });
    return Promise.resolve(answer(url, method));
  });
}

export function mount(ui: ReactNode, route = "/") {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  // Components read the trace the feature layout's spine loads; some fetch it themselves.
  store.set(traceAtom("demo"), { trace: kit.trace, error: null });
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[route]}>{ui}</MemoryRouter>
    </Provider>,
  );
  return store;
}

export const settlePosts = () =>
  kit.calls
    .filter((call) => call.method === "POST" && call.url.pathname === "/api/settle")
    .map((call) => call.url.searchParams);
