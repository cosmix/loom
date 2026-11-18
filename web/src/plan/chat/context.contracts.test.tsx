import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { routes } from "@/plan/router";
import { checklistAtom, planSocketAtom } from "@/plan/state/atoms";

type Obj = Record<string, unknown>;
type Attached = { kind: string; path: string; quote: string };
type ChatCommand = Obj & { client_id: string; text: string | null; attachments: Attached[] };

const SECTION_PAGE = "/f/atlas/product/section_05";
const SECTION_PATH = "doc/product/atlas/section_05.md";
const SECTION_BODY = "Operators review one section at a time.";
const OFFER_LABEL = "Write section 06";
const FIRST_SECTION_PROMPT = "What does this section decide?";
const TYPED = "Is this ready to plan against?";
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const NARROW_MEDIA = window.matchMedia;

// Plan 2's socket test seam: connectPlanSocket with a WebSocket stand-in.
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
  open = (): void => {
    this.readyState = 1;
    this.onopen?.(new Event("open"));
  };
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

/** Any document the page or the composer asks for, carrying the path it was asked for. */
function docAt(rel: string): Obj {
  const path = rel.startsWith("doc/") ? rel : `doc/${rel}`;
  return {
    ...docFixture,
    path,
    feature: "atlas",
    sections: [],
    text: `# Doc\n\n${SECTION_BODY}\n`,
  };
}

function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    if (url.pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    if (url.pathname === "/api/hashes") return Promise.resolve(json({ head: "c1", docs: {} }));
    if (url.pathname === "/api/server") return Promise.resolve(json(hello().server));
    if (url.pathname === "/api/doc") {
      return Promise.resolve(json(docAt(url.searchParams.get("path") ?? "")));
    }
    return Promise.resolve(json({ error: "not_found", message: "no such page" }, 404));
  });
}

/** Section 05 written and section 06 the primary, so the panel has an offer it could still draw. */
function atlasChecklist(): Obj {
  const base = structuredClone(helloFixture.checklists[0]!);
  const section = (number: string, written: boolean) => ({
    ...base.sections[0],
    number,
    written,
  });
  const primary = {
    ...base.primary,
    id: "write-section:06",
    kind: "write_section",
    label: OFFER_LABEL,
    target: "doc/product/atlas/section_06.md",
    feature: "atlas",
    number: 6,
  };
  return {
    ...base,
    feature: "atlas",
    sections: [section("05", true), section("06", false)],
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    plan_rows: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    primary,
    secondary: [],
    items: [],
  };
}

function hello(): Obj & { server: Obj } {
  const base = structuredClone(helloFixture);
  const server = { ...base.server, read_only: [], you: ADA };
  const holder = { ...ADA, away_since: null, frees_at: null };
  const checklists = [atlasChecklist()];
  const fields = { you: ADA, server, presence: { entries: [] }, authoring: { holder }, checklists };
  return { ...base, ...fields };
}

function snapshot(session: Obj | null, turns: Obj[] = []): Obj {
  const base = structuredClone(snapshotFixture);
  const fields = { ...base, session, turns, queue: [], previous: null };
  return { type: "chat", frame: "snapshot", ...fields };
}

/** The session the server reports for a chat this feature's screen started. */
function atlasSession(state: string): Obj {
  const fields = { state, holder: ADA.name, queued: 0, feature: "atlas", plan: null };
  return { ...structuredClone(snapshotFixture.session), ...fields };
}

const seed = (...frames: unknown[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

/** Mounts the real routes and panel on `path` with no session, as a fresh page load would. */
async function mount(path: string) {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  FakeSocket.last = undefined;
  const store = createStore();
  initClient(store);
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  disposers.push(handle.dispose);
  store.set(planSocketAtom, handle);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(<Provider store={store}>{<RouterProvider router={router} />}</Provider>);
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => FakeSocket.last?.open());
  seed(hello(), snapshot(null));
  return { store, router, chat: await panel() };
}

const panel = (): Promise<HTMLElement> => screen.findByRole("complementary", { name: "Chat" });
const composer = (chat: HTMLElement): Promise<HTMLTextAreaElement> =>
  within(chat).findByRole("textbox", { name: "Message" }) as Promise<HTMLTextAreaElement>;
const contextLine = (chat: HTMLElement, name: string): Promise<HTMLElement> =>
  within(chat).findByLabelText(`Viewing ${name}`);
const text = (element: Element): string => (element.textContent ?? "").replace(/\s+/g, " ").trim();
const disabled = (element: HTMLElement): boolean =>
  element.hasAttribute("disabled") || element.getAttribute("aria-disabled") === "true";
const commandsNamed = (command: string): ChatCommand[] =>
  (FakeSocket.last?.sent ?? []).filter(
    (sent) => sent.type === "chat" && sent.command === command,
  ) as ChatCommand[];

/** Time for anything a click set off to reach the socket. */
const settle = (): Promise<void> =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });

async function typeAndSend(chat: HTMLElement, message: string): Promise<void> {
  fireEvent.change(await composer(chat), { target: { value: message } });
  const send = within(chat).getByRole("button", { name: "Send" });
  await waitFor(() => expect(disabled(send)).toBe(false));
  fireEvent.click(send);
}

async function sentCommand(command: string): Promise<ChatCommand> {
  await waitFor(() => expect(commandsNamed(command)).toHaveLength(1));
  return commandsNamed(command)[0]!;
}

/** Answers the start command and reports the running chat, its first message carrying `attached`. */
function acceptStart(start: ChatCommand): void {
  const attachment = snapshotFixture.turns[0]!.items[0]!.attachments![0]!;
  const attachments = start.attachments.map((sent) => ({ ...attachment, path: sent.path }));
  const user = { ...snapshotFixture.turns[0]!.items[0]!, text: start.text, attachments };
  const turn = { ...snapshotFixture.turns[0]!, id: "t1", open: false, changed_files: [] };
  seed(
    { type: "chat", frame: "result", client_id: start.client_id, ok: true, error: null },
    snapshot(atlasSession("idle"), [{ ...turn, items: [user] }]),
  );
}

beforeEach(() => {
  localStorage.clear();
  stubServer();
  // Wide viewport: the panel is a docked region at lg and above, a sheet below.
  const value = (media: string) => ({ ...NARROW_MEDIA(media), matches: /min-width/.test(media) });
  Object.defineProperty(window, "matchMedia", { configurable: true, value });
});

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  cleanup();
  vi.unstubAllGlobals();
});

it("the context line follows the route", async () => {
  const { router, chat } = await mount(SECTION_PAGE);
  await contextLine(chat, "section_05");
  expect((await composer(chat)).placeholder).toContain("section_05");

  // Nothing is clicked between the two pages: the line follows the navigation on its own.
  await act(() => router.navigate("/f/atlas/metaplan"));

  await contextLine(chat, "metaplan");
  expect(within(chat).queryByLabelText("Viewing section_05")).toBeNull();
  const placeholder = (await composer(chat)).placeholder;
  expect(placeholder).toContain("metaplan");
  expect(placeholder).not.toContain("section_05");
});

it("sending attaches the viewed document", async () => {
  const { chat } = await mount(SECTION_PAGE);
  await typeAndSend(chat, TYPED);

  const start = await sentCommand("start");
  expect(start.text).toBe(TYPED);
  expect(start.attachments.map((sent) => [sent.kind, sent.path])).toEqual([
    ["source", SECTION_PATH],
  ]);
  expect(start.attachments[0]!.quote).toContain(SECTION_BODY);

  // The next message of the same chat, on the same page, has already told the chat all this.
  acceptStart(start);
  await typeAndSend(chat, "And what about the second part?");
  const next = await sentCommand("send");
  expect(next.attachments.map((sent) => sent.path)).not.toContain(SECTION_PATH);
});

it("a dismissed context is not attached", async () => {
  const { chat } = await mount(SECTION_PAGE);
  await contextLine(chat, "section_05");
  fireEvent.click(within(chat).getByRole("button", { name: "Dismiss context" }));
  await waitFor(() => expect(within(chat).queryByLabelText("Viewing section_05")).toBeNull());
  expect((await composer(chat)).placeholder).not.toContain("section_05");

  await typeAndSend(chat, TYPED);

  const start = await sentCommand("start");
  expect(start.text).toBe(TYPED);
  expect(start.attachments.map((sent) => sent.path)).not.toContain(SECTION_PATH);
});

it("clicking a prompt fills the composer and sends nothing", async () => {
  const { chat } = await mount(SECTION_PAGE);
  const list = await within(chat).findByRole("list", { name: "Suggested prompts" });
  const first = within(list).getAllByRole("button")[0]!;
  expect(text(first)).toBe(FIRST_SECTION_PROMPT);

  fireEvent.click(first);

  const box = await composer(chat);
  await waitFor(() => expect(box.value).toBe(FIRST_SECTION_PROMPT));
  await waitFor(() => expect(document.activeElement).toBe(box));
  await settle();
  expect(FakeSocket.last?.sent.filter((sent) => sent.type === "chat")).toEqual([]);
});

it("the chat panel draws no workflow offer", async () => {
  const { store, chat } = await mount(SECTION_PAGE);
  await composer(chat);
  await waitFor(() => expect(store.get(checklistAtom)?.primary?.kind).toBe("write_section"));
  await settle();
  expect(within(chat).queryByRole("button", { name: OFFER_LABEL })).toBeNull();
  expect(text(chat)).not.toContain(OFFER_LABEL);

  // A concluded chat offers New chat and the prompts, and the spine keeps the offer.
  seed(snapshot(atlasSession("ended")));
  await within(chat).findByRole("button", { name: /new chat/i });
  await settle();
  expect(within(chat).queryByRole("button", { name: OFFER_LABEL })).toBeNull();
  expect(text(chat)).not.toContain(OFFER_LABEL);
});
