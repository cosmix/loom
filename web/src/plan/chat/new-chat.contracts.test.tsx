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
import { planSocketAtom } from "@/plan/state/atoms";

// Helpers copied from context.contracts.test.tsx (a frozen file is never imported from).
type Obj = Record<string, unknown>;
type Attached = { kind: string; path: string; quote: string };
type ChatCommand = Obj & {
  client_id: string;
  text: string | null;
  offer: string | null;
  attachments: Attached[];
};

const SECTION_PAGE = "/f/atlas/product/section_05";
const SECTION_BODY = "Operators review one section at a time.";
const OFFER_LABEL = "Write section 06";
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

/** An ended chat whose one closed turn holds the user's question and the reply. */
function endedChat(): { session: Obj; turns: Obj[] } {
  const base = structuredClone(snapshotFixture.turns[0]!);
  const question = {
    ...base.items[0]!,
    text: "Earlier question",
    attachments: [],
    origin: null,
    delivery: "delivered",
  };
  const reply = base.items.find((entry) => entry.type === "text")!;
  const answer = { ...reply, id: "a-earlier", markdown: "Earlier answer" };
  const earlier = { ...base, id: "t1", open: false, changed_files: [], items: [question, answer] };
  return { session: atlasSession("ended"), turns: [earlier] };
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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("New chat on an ended chat starts no session until the first send", async () => {
  const { chat } = await mount(SECTION_PAGE);
  seed({ ...snapshot(null), previous: endedChat() });
  await within(chat).findByText("Earlier answer");

  fireEvent.click(within(chat).getByRole("button", { name: /new chat/i }));
  await settle();
  expect(commandsNamed("start")).toHaveLength(0);
  await waitFor(() => expect(text(chat)).not.toContain("Earlier answer"));
  const box = await composer(chat);
  await waitFor(() => expect(document.activeElement).toBe(box));
  expect(box.value).toBe("");

  // The first message is the session's start.
  await typeAndSend(chat, "Plan the rollout");
  const start = await sentCommand("start");
  expect(start.text).toBe("Plan the rollout");
  expect(start.offer).toBeNull();
});

it("New chat on a live chat ends it and starts nothing", async () => {
  const { chat } = await mount(SECTION_PAGE);
  seed(snapshot(atlasSession("idle"), [endedChat().turns[0]!]));
  await within(chat).findByText("Earlier answer");
  vi.spyOn(window, "confirm").mockReturnValue(true);

  fireEvent.click(within(chat).getByRole("button", { name: "New chat" }));
  const end = await sentCommand("end");
  seed(
    { type: "chat", frame: "result", client_id: end.client_id, ok: true, error: null },
    { ...snapshot(null), previous: endedChat() },
  );

  await waitFor(() => expect(text(chat)).not.toContain("Earlier answer"));
  const box = await composer(chat);
  await waitFor(() => expect(document.activeElement).toBe(box));
  await settle();
  expect(commandsNamed("start")).toHaveLength(0);
});
