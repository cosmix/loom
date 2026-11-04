import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import conceptFixture from "@/plan/api/fixtures/concept-summary.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { routes } from "@/plan/router";
import { planSocketAtom } from "@/plan/state/atoms";

type Obj = Record<string, unknown>;
type Attached = { kind: string; path: string };
type ChatCommand = Obj & { text: string | null; attachments: Attached[] };

const SECTION_PAGE = "/f/atlas/product/section_05";
const SOURCE_PAGE = "/f/atlas/source/product/atlas/section_05.md";
const SECTION_PATH = "doc/product/atlas/section_05.md";
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const NARROW_MEDIA = window.matchMedia;

// The socket test seam the other route-level chat tests use: connectPlanSocket over a stand-in.
class FakeSocket {
  static last: FakeSocket | undefined;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  sent: Obj[] = [];
  constructor() {
    FakeSocket.last = this;
  }
  send = (data: string): number => this.sent.push(JSON.parse(data) as Obj);
  close = (): number => (this.readyState = 3);
  receive = (frame: unknown): void =>
    this.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent);
}

let disposers: Array<() => void> = [];

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function hello(): Obj {
  const base = structuredClone(helloFixture);
  const server = { ...base.server, read_only: [], you: ADA };
  const holder = { ...ADA, away_since: null, frees_at: null };
  const first = base.checklists[0]!;
  const section = (number: string) => ({ ...first.sections[0], number, written: true });
  const checklist = {
    ...base.checklists[0],
    feature: "atlas",
    has_product: true,
    sections: [section("05")],
    metaplans: [],
    plan_rows: [],
    primary: null,
    secondary: [],
    items: [],
  };
  const fields = { you: ADA, server, presence: { entries: [] }, authoring: { holder } };
  return { ...base, ...fields, checklists: [checklist] };
}

function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    if (url.pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    if (url.pathname === "/api/hashes") return Promise.resolve(json({ head: "c1", docs: {} }));
    if (url.pathname === "/api/server") return Promise.resolve(json(hello().server));
    if (url.pathname === "/api/concept/atlas/product/section_05") {
      const page = { ...conceptFixture, key: "atlas/product/section_05", path: SECTION_PATH };
      return Promise.resolve(json(page));
    }
    if (url.pathname === "/api/doc") {
      const path = url.searchParams.get("path") ?? "";
      const text = "# Doc\n\nBody.\n";
      const body = { ...docFixture, path, feature: "atlas", sections: [], text };
      return Promise.resolve(json(body));
    }
    return Promise.resolve(json({ error: "not_found", message: "no such page" }, 404));
  });
}

const seed = (...frames: unknown[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

async function mount(path: string) {
  disposers.splice(0).forEach((dispose) => dispose());
  FakeSocket.last = undefined;
  const store = createStore();
  initClient(store);
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  disposers.push(handle.dispose);
  store.set(planSocketAtom, handle);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => {
    FakeSocket.last!.readyState = 1;
    FakeSocket.last!.onopen?.(new Event("open"));
  });
  const empty = { session: null, turns: [], queue: [], previous: null };
  const snapshot = { ...structuredClone(snapshotFixture), ...empty };
  seed(hello(), { type: "chat", frame: "snapshot", ...snapshot });
  const chat = await screen.findByRole("complementary", { name: "Chat" });
  return { router, chat };
}

const sentStart = (): ChatCommand[] =>
  (FakeSocket.last?.sent ?? []).filter(
    (sent) => sent.type === "chat" && sent.command === "start",
  ) as ChatCommand[];

async function openSource(chat: HTMLElement) {
  await within(chat).findByLabelText("Viewing section_05");
  const main = document.querySelector<HTMLElement>("main") ?? document.body;
  fireEvent.click(await within(main).findByRole("link", { name: "Source" }));
  return within(chat).findByLabelText("Viewing section_05.md");
}

async function send(chat: HTMLElement): Promise<void> {
  const found = await within(chat).findByRole("textbox", { name: "Message" });
  const box = found as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: "Is this ready?" } });
  const button = within(chat).getByRole("button", { name: "Send" });
  await waitFor(() => expect(button.hasAttribute("disabled")).toBe(false));
  fireEvent.click(button);
  await waitFor(() => expect(sentStart()).toHaveLength(1));
}

beforeEach(() => {
  localStorage.clear();
  stubServer();
  const value = (media: string) => ({ ...NARROW_MEDIA(media), matches: /min-width/.test(media) });
  Object.defineProperty(window, "matchMedia", { configurable: true, value });
});

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  cleanup();
  vi.unstubAllGlobals();
});

it("a Source page keeps the viewed document and sends it", async () => {
  const { router, chat } = await mount(SECTION_PAGE);
  await openSource(chat);
  expect(router.state.location.pathname).toBe(SOURCE_PAGE);

  await send(chat);

  expect(sentStart()[0]!.attachments.map((sent) => [sent.kind, sent.path])).toEqual([
    ["source", SECTION_PATH],
  ]);
});

it("a Source page's dismissed context sends no attachment", async () => {
  const { chat } = await mount(SECTION_PAGE);
  const line = await openSource(chat);
  fireEvent.click(within(line).getByRole("button", { name: "Dismiss context" }));
  await waitFor(() => expect(within(chat).queryByLabelText("Viewing section_05.md")).toBeNull());

  await send(chat);

  expect(sentStart()[0]!.attachments).toEqual([]);
});
