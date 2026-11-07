import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { authoringViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";
import { PageHeading } from "@/plan/concept/page-heading";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { SettleControl } from "./settle-control";
import { traceAtom } from "./state";

const S02 = "doc/product/demo/section_02.md";
const S03 = "doc/product/demo/section_03.md";
const HASH = "sha256:5e771ed0c0ffee";
const STALE_MESSAGE = "the planning view is catching up with the document; try again in a moment";
const UNRESOLVED = {
  error: "unresolved",
  message: "unresolved points",
  my_calls: 2,
  open_questions: 1,
};

type Answer = { status: number; body: unknown };

let posts: URLSearchParams[];
let trace: TraceView;
let settleAnswers: Answer[];
/** When set, the next settle request waits for `release()` before it answers. */
let hold: { release: () => void } | null;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function settleResponse(): Promise<Response> {
  const next = settleAnswers.shift() ?? { status: 200, body: savedBody() };
  if (next.status === 200) {
    const doc = trace.documents.find((entry) => entry.path === S03);
    if (doc) doc.settle = "settled";
  }
  if (!hold) return Promise.resolve(json(next.body, next.status));
  return new Promise((resolve) => {
    hold = { release: () => resolve(json(next.body, next.status)) };
  });
}

function savedBody() {
  return { result: "saved", hash: "sha256:after", validation: null };
}

beforeEach(() => {
  posts = [];
  settleAnswers = [];
  hold = null;
  trace = traceViewSchema.parse(structuredClone(traceFixture));
  const draft = trace.documents.find((entry) => entry.path === S03);
  // Settled here as a written draft; the fixture lists it unwritten, which nothing settles.
  if (draft) Object.assign(draft, { settle: "draft", progress: "written", settleable: true });
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    if (url.pathname === "/api/trace/demo") return Promise.resolve(json(structuredClone(trace)));
    if (url.pathname === "/api/settle" && init?.method === "POST") {
      posts.push(url.searchParams);
      return settleResponse();
    }
    if (url.pathname === "/api/doc") {
      return Promise.resolve(
        json({ ...docFixture, path: url.searchParams.get("path"), hash: HASH }),
      );
    }
    return Promise.resolve(json({ error: "not_found", message: "no such route" }, 404));
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mount(ui: ReactNode) {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(traceAtom("demo"), { trace, error: null });
  return render(ui, {
    wrapper: ({ children }) => (
      <Provider store={store}>
        <MemoryRouter>{children}</MemoryRouter>
      </Provider>
    ),
  });
}

const settle = (path: string) => <SettleControl feature="demo" path={path} hash={HASH} />;

it("a stale answer shows the server's message", async () => {
  settleAnswers = [{ status: 409, body: { error: "stale", message: STALE_MESSAGE } }];
  mount(settle(S03));
  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  const notice = await screen.findByRole("alert");
  expect(notice.textContent).toBe(STALE_MESSAGE);
});

it("an unresolved answer for a document left behind does not open a dialog on the next", async () => {
  settleAnswers = [{ status: 409, body: UNRESOLVED }];
  hold = { release: () => {} };
  const view = mount(settle(S03));
  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  view.rerender(settle(S02));
  await screen.findByRole("button", { name: "Changed since you settled it" });
  hold?.release();
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("a summary page posts the source document's hash, not its summary file's", async () => {
  const page = conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    key: "demo/product/section_03",
    path: S03,
    feature: "demo",
    state: null,
    implemented_by: [],
    file_hash: "4b8d0e2f6a1c7395",
  });
  mount(<PageHeading page={page} />);
  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0]!.get("hash")).toBe(HASH);
});

it("focus stays on the button once Mark settled turns it into Settled", async () => {
  mount(settle(S03));
  const button = await screen.findByRole("button", { name: "Mark settled" });
  fireEvent.click(button);
  await screen.findByRole("button", { name: "Settled" });
  expect(screen.queryByRole("button", { name: "Mark settled" })).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(button));
});

it("focus stays on the button after a confirmed settle closes the popover", async () => {
  settleAnswers = [{ status: 409, body: UNRESOLVED }];
  mount(settle(S03));
  const button = await screen.findByRole("button", { name: "Mark settled" });
  fireEvent.click(button);
  const dialog = await screen.findByRole("dialog", { name: "Settle with open items?" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Settle anyway" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(button));
  expect(posts.map((params) => params.get("confirm"))).toEqual([null, "1"]);
});

it("Not yet closes the popover and returns focus to the Mark settled button", async () => {
  settleAnswers = [{ status: 409, body: UNRESOLVED }];
  mount(settle(S03));
  const opener = await screen.findByRole("button", { name: "Mark settled" });
  fireEvent.click(opener);
  const dialog = await screen.findByRole("dialog");
  fireEvent.click(within(dialog).getByRole("button", { name: "Not yet" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(opener));
  expect(posts).toHaveLength(1);
});

it("Mark settled after Not yet asks the server again, unconfirmed, and confirms again", async () => {
  settleAnswers = [
    { status: 409, body: UNRESOLVED },
    { status: 409, body: UNRESOLVED },
  ];
  mount(settle(S03));
  const opener = await screen.findByRole("button", { name: "Mark settled" });
  fireEvent.click(opener);
  fireEvent.click(
    within(await screen.findByRole("dialog")).getByRole("button", { name: "Not yet" }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  fireEvent.click(opener);
  await screen.findByRole("dialog", { name: "Settle with open items?" });
  expect(posts.map((params) => params.get("confirm"))).toEqual([null, null]);
});

it("Escape dismisses the confirmation and returns focus to the button", async () => {
  settleAnswers = [{ status: 409, body: UNRESOLVED }];
  mount(settle(S03));
  const opener = await screen.findByRole("button", { name: "Mark settled" });
  fireEvent.click(opener);
  const dialog = await screen.findByRole("dialog");
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(opener));
  expect(posts).toHaveLength(1);
});

it("a locked prop hides the controls whatever the file name says", async () => {
  mount(<SettleControl feature="demo" path={S03} hash={HASH} locked />);
  await screen.findByText("Draft");
  expect(screen.queryByRole("button", { name: "Mark settled" })).toBeNull();
});
