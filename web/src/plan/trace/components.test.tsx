import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { sentenceSchema } from "@/plan/api/schema-concept";
import { traceViewSchema, type TraceDoc, type TraceView } from "@/plan/api/schema-trace";
import { docChangedAtom } from "@/plan/files/atoms";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { EdgeChips } from "./edge-chips";
import { ImpactRows } from "./impact-rows";
import { SettleControl } from "./settle-control";
import { traceAtom } from "./state";
import { StubPanel } from "./stub-panel";

// The comparison's diff view loads CodeMirror; these tests read the two texts it is given.
vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: ({ before, after }: { before: string | null; after: string | null }) => (
    <pre data-testid="diff">{`${before} => ${after}`}</pre>
  ),
}));

const VISION = "doc/product/demo/index.md";
const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";
const S03 = "doc/product/demo/section_03.md";
const METAPLAN = "doc/plans/demo/index.md";
const FIRST_PLAN = "doc/plans/demo/PLAN-first.md";
const HASH = "sha256:5e771ed0c0ffee";
const COMMIT = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";

type Call = { url: URL; method: string };
type Answer = { status: number; body: unknown };

let calls: Call[];
let trace: TraceView;
let settleAnswers: Answer[];
let settledCommit: string | null;

const CURRENT_TEXT = "current text";
const VISION_TEXT = [
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

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function docAnswer(rel: string): unknown {
  const text =
    rel === "product/demo/index.md"
      ? VISION_TEXT
      : rel === "plans/demo/index.md"
        ? METAPLAN_TEXT
        : CURRENT_TEXT;
  const sections = rel === "plans/demo/index.md" ? METAPLAN_SECTIONS : [];
  return { ...docFixture, path: `doc/${rel}`, hash: HASH, text, sections };
}

function answer(url: URL, method: string): Response {
  const { pathname, searchParams } = url;
  if (pathname === "/api/trace/demo") return json(trace);
  if (pathname === "/api/settle" && method === "POST") {
    const next = settleAnswers.shift();
    return next
      ? json(next.body, next.status)
      : json({ result: "saved", hash: "sha256:after", validation: null });
  }
  if (pathname === "/api/settled-version") return json({ commit: settledCommit });
  if (pathname === "/api/at-commit") {
    return json({ path: searchParams.get("path"), text: "settled text" });
  }
  if (pathname === "/api/doc") return json(docAnswer(searchParams.get("path") ?? ""));
  return json({ error: "not_found", message: "no such route" }, 404);
}

function patchDoc(path: string, patch: Partial<TraceDoc>): void {
  const found = trace.documents.find((doc) => doc.path === path);
  if (!found) throw new Error(`the trace fixture holds no ${path}`);
  Object.assign(found, patch);
}

beforeEach(() => {
  calls = [];
  settleAnswers = [];
  settledCommit = COMMIT;
  trace = traceViewSchema.parse(structuredClone(traceFixture));
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    const method = init?.method ?? "GET";
    calls.push({ url, method });
    return Promise.resolve(answer(url, method));
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mount(ui: ReactNode, route = "/") {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  // Chips read the trace the feature layout's spine loads; the other parts fetch it themselves.
  store.set(traceAtom("demo"), { trace, error: null });
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[route]}>{ui}</MemoryRouter>
    </Provider>,
  );
  return store;
}

const settlePosts = () =>
  calls
    .filter((call) => call.method === "POST" && call.url.pathname === "/api/settle")
    .map((call) => call.url.searchParams);

const buttons = () => screen.queryAllByRole("button").map((button) => button.textContent);

async function control(path: string) {
  const store = mount(<SettleControl feature="demo" path={path} hash={HASH} />);
  await screen.findByText(/^(Draft|Revised|Stub changed|Upstream changed|Settled)$/);
  return store;
}

const STATES = [
  ["draft", "Draft", ["Mark settled"], false],
  ["revised", "Revised", ["Mark settled", "Unsettle"], true],
  ["stub_changed", "Stub changed", ["Mark settled", "Unsettle"], false],
  ["upstream_changed", "Upstream changed", ["Mark settled", "Unsettle"], false],
  ["settled", "Settled", ["Unsettle"], false],
] as const;

for (const [state, label, expected, link] of STATES) {
  it(`a ${state} document shows ${label} and offers ${expected.join(" and ")}`, async () => {
    patchDoc(S03, { settle: state });
    await control(S03);
    expect(screen.getByText(label)).toBeTruthy();
    expect(buttons()).toEqual(expected);
    expect(screen.queryByRole("link", { name: "Changes since settled" }) !== null).toBe(link);
  });
}

it("a locked document shows its state and no controls", async () => {
  const locked = "doc/plans/demo/DONE-PLAN-first.md";
  patchDoc(FIRST_PLAN, { path: locked, settle: "settled" });
  await control(locked);
  expect(buttons()).toEqual([]);
  expect(screen.queryByRole("link", { name: "Changes since settled" })).toBeNull();
});

it("Unsettle posts clear=1 with the document and its hash", async () => {
  patchDoc(S03, { settle: "settled" });
  await control(S03);
  fireEvent.click(screen.getByRole("button", { name: "Unsettle" }));
  await waitFor(() => expect(settlePosts()).toHaveLength(1));
  const [params] = settlePosts();
  expect(params!.get("clear")).toBe("1");
  expect(params!.get("path")).toBe("product/demo/section_03.md");
  expect(params!.get("hash")).toBe(HASH);
});

it("the unresolved dialog counts both kinds, and cancelling resends nothing", async () => {
  settleAnswers = [
    {
      status: 409,
      body: { error: "unresolved", message: "unresolved points", my_calls: 2, open_questions: 1 },
    },
  ];
  await control(S03);
  fireEvent.click(screen.getByRole("button", { name: "Mark settled" }));
  const dialog = await screen.findByRole("dialog");
  expect(dialog.textContent).toContain("2 My calls and 1 open question");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(settlePosts()).toHaveLength(1);
});

it("a conflict reloads the document and the trace and asks to settle again", async () => {
  settleAnswers = [
    { status: 409, body: { result: "conflict", current_text: "x", current_hash: "sha256:newer" } },
  ];
  const store = await control(S03);
  fireEvent.click(screen.getByRole("button", { name: "Mark settled" }));
  await screen.findByText(/changed while you were reading it/);
  expect(store.get(docChangedAtom)).toEqual({ [S03]: "sha256:newer" });
  // The trace was fetched on mount and again after the answer.
  await waitFor(() =>
    expect(calls.filter((call) => call.url.pathname === "/api/trace/demo")).toHaveLength(2),
  );
});

it("a settled line that was never committed says so and opens no comparison", async () => {
  settledCommit = null;
  patchDoc(S02, { settle: "revised" });
  await control(S02);
  fireEvent.click(screen.getByRole("link", { name: "Changes since settled" }));
  await screen.findByText("The settled version is not committed");
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("changes since settled compares the settled commit with the file now", async () => {
  patchDoc(S02, { settle: "revised" });
  await control(S02);
  fireEvent.click(screen.getByRole("link", { name: "Changes since settled" }));
  const diff = await screen.findByTestId("diff");
  expect(diff.textContent).toBe(`settled text => ${CURRENT_TEXT}`);
  const atCommit = calls.find((call) => call.url.pathname === "/api/at-commit");
  expect(atCommit?.url.searchParams.get("commit")).toBe(COMMIT);
});

function sentenceOf(path: string, anchor: string) {
  const at = { path, anchor };
  return sentenceSchema.parse({
    id: "s1",
    text: "A sentence.",
    marks: [],
    citations: [{ kind: "source", ...at }],
    rests_on: [at],
    plans: [],
  });
}

it("a product sentence shows the plans and entries covering its headings and open findings", async () => {
  mount(<EdgeChips sentence={sentenceOf(S01, "two")} summary="demo/product/section_01" />);
  const group = await screen.findByRole("group", { name: "Related documents" });
  const links = within(group).getAllByRole("link");
  expect(links.map((link) => link.textContent)).toEqual(["first · build", "1 open finding"]);
  expect(links[0]!.getAttribute("href")).toBe(
    "/f/demo/source/plans/demo/PLAN-first.md#stage%3Abuild",
  );
  expect(links[1]!.getAttribute("href")).toBe("/f/demo/findings");
});

it("a plan sentence shows the spec headings the cited stage implements, with their state", async () => {
  mount(
    <EdgeChips sentence={sentenceOf(FIRST_PLAN, "stage:build")} summary="plans/first" />,
    "/f/demo/engineering/first",
  );
  const group = await screen.findByRole("group", { name: "Related documents" });
  const [chip] = within(group).getAllByRole("link");
  expect(chip!.textContent).toBe("Two Settled");
  expect(chip!.getAttribute("href")).toBe("/f/demo/source/product/demo/section_01.md#two");
});

it("a sentence with nothing around it draws no chips", () => {
  mount(<EdgeChips sentence={sentenceOf(S03, "none")} summary="demo/product/section_03" />);
  expect(screen.queryByRole("group", { name: "Related documents" })).toBeNull();
});

it("impact rows list the plan covering a changed section with its state", async () => {
  patchDoc(FIRST_PLAN, { settle: "upstream_changed" });
  mount(<ImpactRows path={S01} />);
  const list = await screen.findByRole("list", { name: "Affected documents" });
  expect(list.textContent).toMatch(/First.*Upstream changed/);
});

it("impact rows list a section that moved when its plan changed", async () => {
  patchDoc(S01, { settle: "revised" });
  mount(<ImpactRows path={FIRST_PLAN} />);
  const list = await screen.findByRole("list", { name: "Affected documents" });
  expect(list.textContent).toMatch(/Revised/);
});

it("impact rows draw nothing for a document outside every feature", async () => {
  mount(<ImpactRows path="doc/loom/knowledge/notes.md" />);
  expect(screen.queryByRole("list")).toBeNull();
});

it("a plan page lists its briefs, each linked to its source", async () => {
  mount(<StubPanel feature="demo" path={FIRST_PLAN} />);
  const list = await screen.findByRole("list", { name: "Briefs" });
  const link = within(list).getByRole("link", { name: "build/r1.md" });
  expect(link.getAttribute("href")).toBe("/f/demo/source/plans/demo/briefs/first/build/r1.md");
});

it("a plan stub shows its metaplan entry's heading and first paragraph as text", async () => {
  mount(<StubPanel feature="demo" path={FIRST_PLAN} />);
  const link = await screen.findByRole("link", { name: "Stub in the metaplan" });
  expect(link.getAttribute("href")).toBe("/f/demo/source/plans/demo/index.md#1-first-plan-firstmd");
  await screen.findByText("Build the first plan from one idea.");
  expect(screen.getByText("1. First (`PLAN-first.md`)")).toBeTruthy();
});

it("a section stub shows its vision row, and a changed stub says so", async () => {
  patchDoc(S02, { settle: "stub_changed" });
  mount(<StubPanel feature="demo" path={S02} />);
  await screen.findByText("What beta promises");
  expect(screen.getByText("Beta")).toBeTruthy();
  expect(screen.getByRole("note").textContent).toBe("Stub changed since settled");
  expect(screen.getByRole("link", { name: "Stub in the vision" }).getAttribute("href")).toContain(
    "/f/demo/source/product/demo/index.md",
  );
});

it("the vision and the metaplan show no stub", async () => {
  mount(
    <>
      <StubPanel feature="demo" path={VISION} />
      <StubPanel feature="demo" path={METAPLAN} />
    </>,
  );
  await waitFor(() =>
    expect(calls.some((call) => call.url.pathname === "/api/trace/demo")).toBe(true),
  );
  expect(screen.queryByRole("link")).toBeNull();
});
