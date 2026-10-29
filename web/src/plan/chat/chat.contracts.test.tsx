import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import deltasFixture from "@/plan/api/fixtures/chat-deltas.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { addAttachmentAtom, type AttachmentDraft } from "@/plan/attachments";
import { docRel } from "@/plan/files/address";
import { routes } from "@/plan/router";
import { planSocketAtom } from "@/plan/state/atoms";

import * as fmt from "./format-attachments";

type Obj = Record<string, unknown>;
type Store = ReturnType<typeof createStore>;
type StartOptions = { turn?: Obj; extra?: Obj; viewer?: boolean; path?: string };

const ATTACHMENTS = join(import.meta.dirname, "../../../../loom/tests/fixtures/plan_session");
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const DANA = { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" };
const READER = { public_id: "aaaaaaaaaaaaaaaa", name: "Reader" };
const ITEMS: Obj[] = snapshotFixture.turns[0].items;
const NARROW_MEDIA = window.matchMedia;
const FILE_PAGE = `/files/${docRel(docFixture.path)}`;

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

let replies: Record<string, unknown> = {};
let fetchMock = vi.fn<typeof fetch>();
let disposers: Array<() => void> = [];
let holderName = "Ada";
let nextId = 0;
function pathOf(input: RequestInfo | URL): string {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost").pathname;
}
// Answers from `replies` by pathname; /api/doc always serves plan 3's doc fixture.
async function answer(input: RequestInfo | URL): Promise<Response> {
  const path = pathOf(input);
  const body = path === "/api/doc" ? docFixture : replies[path];
  const status = body === undefined ? 404 : 200;
  const json = JSON.stringify(body ?? { error: "not_found", message: "not found" });
  return new Response(json, { status, headers: { "content-type": "application/json" } });
}
function posts(path: string): string[] {
  return fetchMock.mock.calls
    .filter(([input, init]) => pathOf(input) === path && init?.method === "POST")
    .map(([, init]) => String(init?.body ?? ""));
}
function hello(viewer: boolean): Obj {
  const base = structuredClone(helloFixture);
  const you = viewer ? READER : ADA;
  const holder = { ...(viewer ? DANA : ADA), away_since: null, frees_at: null };
  const server = { ...base.server, read_only: [], you };
  return { ...base, you, server, presence: { entries: [] }, authoring: { holder } };
}
function item(type: string, fields: Obj = {}): Obj {
  const n = (nextId += 1);
  const base = structuredClone(ITEMS.find((candidate) => candidate.type === type)) as Obj;
  const ids: Obj = { id: `${type}-${n}`, tool_use_id: `toolu_${n}`, request_id: `r${n}` };
  const own = Object.fromEntries(Object.entries(ids).filter(([key]) => key in base));
  return { ...base, ...own, ...fields };
}
const tool = (name: string, fields: Obj = {}): Obj =>
  item("tool", { name, status: "done", result: "ok", patch: null, ...fields });
const bashPermission = (command: string, fields: Obj = {}): Obj =>
  item("permission", { tool: "Bash", input: JSON.stringify({ command }, null, 2), ...fields });
const changedFiles = (paths: string[]): Obj => ({
  changed_files: paths.map((path) => ({ path, before_hash: "a1b2c3d4", by_tool: true })),
});
function draft(path: string, text: string): AttachmentDraft {
  return { kind: "source", path, anchor: null, text, lines: null, stageId: null, hash: "h" };
}
const seed = (...frames: unknown[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));
function snapshot(state: string | null, turns: Obj[], extra: Obj = {}): Obj {
  const base = structuredClone(snapshotFixture);
  const session = state ? { ...base.session, state, holder: holderName, queued: 0 } : null;
  const fields = { ...base, session, turns, queue: [], previous: null, ...extra };
  return { type: "chat", frame: "snapshot", ...fields };
}
// Mounts plan 2's real routes on `path` with a fresh store, as a fresh page load would.
async function mount(path: string, viewer = false): Promise<Store> {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  holderName = viewer ? "Dana" : "Ada";
  replies["/api/server"] = hello(viewer).server;
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
  seed(hello(viewer));
  return store;
}

// Mounts, then seeds one turn holding a user message and `items` (no turn when `items` is empty).
async function start(state: string | null, items: Obj[] = [], options: StartOptions = {}) {
  const store = await mount(options.path ?? "/f/demo/product", options.viewer);
  const open = state === "working" || state === "permission";
  const user = item("user", { text: "Go on", attachments: [], origin: null });
  const turn = { ...structuredClone(snapshotFixture.turns[0]), id: "t1", open, changed_files: [] };
  const turns = items.length ? [{ ...turn, items: [user, ...items], ...options.turn }] : [];
  seed(snapshot(state, turns, options.extra));
  return { store, chat: await panel() };
}
const panel = (): Promise<HTMLElement> => screen.findByRole("complementary", { name: "Chat" });
const logOf = (chat: HTMLElement): Promise<HTMLElement> => within(chat).findByRole("log");
const composer = (chat: HTMLElement): Promise<HTMLTextAreaElement> =>
  within(chat).findByRole("textbox", { name: "Message" }) as Promise<HTMLTextAreaElement>;
const sendButton = (chat: HTMLElement) => within(chat).getByRole("button", { name: "Send" });
const footerOf = async (started: Promise<{ chat: HTMLElement }>) =>
  within((await started).chat).findByRole("group", { name: "Turn 1 changes" });
const text = (element: Element): string => (element.textContent ?? "").replace(/\s+/g, " ");
const disabled = (element: HTMLElement): boolean =>
  element.hasAttribute("disabled") || element.getAttribute("aria-disabled") === "true";
const chatCommands = (): Obj[] => (FakeSocket.last?.sent ?? []).filter((m) => m.type === "chat");
const addDraft = (store: Store, added: AttachmentDraft) =>
  act(() => store.set(addAttachmentAtom, added));
const stored = (): string =>
  Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index) ?? "")
    .map((key) => localStorage.getItem(key) ?? "")
    .join("\n");
function chipsNamed(chat: HTMLElement, pattern: RegExp): HTMLElement[] {
  const name = (accessible: string) => pattern.test(accessible) && !/remove/i.test(accessible);
  return within(chat).queryAllByRole("button", { name });
}
// The smallest ancestor of `element` whose text contains `needle`.
function ancestorWith(element: Element, needle: string): Element {
  let current: Element = element;
  while (!text(current).includes(needle) && current.parentElement) current = current.parentElement;
  return current;
}
async function pinnedCard(chat: HTMLElement): Promise<HTMLElement> {
  const log = await logOf(chat);
  const cards = () => within(chat).queryAllByRole("region", { name: "Permission: Bash" });
  const pinned = () => cards().find((card) => !log.contains(card));
  return waitFor(() => pinned() ?? Promise.reject(new Error("no card outside the transcript")));
}

beforeEach(() => {
  localStorage.clear();
  nextId = 0;
  replies = { "/api/tree": treeFixture, "/api/concept/overview/demo/product": overviewFixture };
  replies["/api/hashes"] = { head: "c1", docs: {} };
  fetchMock = vi.fn<typeof fetch>(answer);
  vi.stubGlobal("fetch", fetchMock);
  // Wide viewport: the panel is a docked region at lg and above, a sheet below.
  const value = (media: string) => ({ ...NARROW_MEDIA(media), matches: /min-width/.test(media) });
  Object.defineProperty(window, "matchMedia", { configurable: true, value });
});

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  window.getSelection()?.removeAllRanges();
  cleanup();
  vi.unstubAllGlobals();
});

it("chat attachment formatter matches the shared attachment fixtures", () => {
  type Case = { text: string; attachments: Parameters<typeof fmt.formatMessage>[1] };
  const read = (name: string) => readFileSync(join(ATTACHMENTS, "attachments", name), "utf8");
  const isCase = (name: string) => name.endsWith(".json") && !name.startsWith("resolve-");
  const names = readdirSync(join(ATTACHMENTS, "attachments")).filter(isCase).sort();
  expect(names).toEqual(expect.arrayContaining(["literal-tag-in-text.json"]));
  expect(names).toEqual(expect.arrayContaining(["quote-cut-long-line.json"]));
  for (const name of names) {
    const input = JSON.parse(read(name)) as Case;
    const delivered = read(name.replace(".json", ".txt"));
    expect(fmt.formatMessage(input.text, input.attachments), name).toBe(delivered);
  }
});

it("chat assistant markdown with raw html renders no html elements", async () => {
  const { chat } = await start("working", [tool("Read")]);
  const markdown = "<script>x()</script><img src=x onerror=y()>\n\nSee [a](javascript:alert(1)).";
  const added = structuredClone(deltasFixture.find((delta) => delta.op === "item_added"));
  seed({ type: "chat", frame: "delta", ...added, item: item("text", { markdown }) });
  const log = await logOf(chat);
  await within(log).findByText(/See/);
  expect(log.querySelector("script, img")).toBeNull();
  expect(log.querySelector('a[href^="javascript:" i]')).toBeNull();
});

it("chat expanded tool row shows html input as text", async () => {
  const input = '{\n  "command": "<b>rm</b>"\n}';
  const { chat } = await start("working", [tool("Bash", { summary: "Run the cleanup", input })]);
  const log = await logOf(chat);
  fireEvent.click(await within(log).findByText("Run the cleanup"));
  const shown = await within(log).findAllByText(/<b>rm<\/b>/);
  expect(log.querySelector("b")).toBeNull();
  expect(shown.some((el) => el.closest("pre, code, [class*='mono']") !== null)).toBe(true);
});

it("chat tool row in error shows the first error line without expanding", async () => {
  const first = "Permission denied: doc/locked.md";
  const failed = { status: "error", error_line: first, result: `${first}\nwhile running the hook` };
  const log = await logOf((await start("working", [tool("Read", failed)])).chat);
  await waitFor(() => expect(text(log)).toContain(first));
  expect(text(log)).not.toContain("while running the hook");
});

it("chat ended turn folds three consecutive tool rows and keeps an error row outside", async () => {
  const patch = (path: string) => `--- a/${path}\n+++ b/${path}\n@@ -1 +1 @@\n-x\n+y\n`;
  const input = (path: string) => JSON.stringify({ file_path: path, old_string: "x" }, null, 2);
  const edit = (path: string) => tool("Edit", { input: input(path), patch: patch(path) });
  const failed = { summary: "Read the missing file", status: "error", error_line: "No such file" };
  const rows = [edit("doc/product/alpha.md"), edit("doc/product/beta.md"), tool("Read")];
  const log = await logOf((await start("idle", [...rows, tool("Read", failed)])).chat);
  await waitFor(() => expect(text(log)).toContain("Used 3 tools · edited 2 files"));
  expect(text(log)).toContain("Read the missing file");
  expect(text(log)).toContain("No such file");
});

it("chat pending permission card shows allow and deny only to the holder", async () => {
  for (const viewer of [false, true]) {
    const { chat } = await start("permission", [bashPermission("ls doc/")], { viewer });
    const card = await pinnedCard(chat);
    const answers = within(card).queryAllByRole("button", { name: /allow|deny/i });
    expect(answers.map((button) => text(button).trim())).toEqual(viewer ? [] : ["Allow", "Deny"]);
    expect(text(card).includes("Waiting for Dana to decide")).toBe(viewer);
  }
});

it("chat permission denied by timeout collapses to the automatic deny line", async () => {
  const denied = bashPermission("ls", { status: "denied", deny_reason: "timeout" });
  const log = await logOf((await start("idle", [denied])).chat);
  await waitFor(() => expect(text(log)).toContain("Denied automatically after one hour: Bash"));
});

it("chat composer refuses slash commands other than compact", async () => {
  const { chat } = await start("idle");
  const box = await composer(chat);
  const note = "Slash commands are not available here";
  fireEvent.change(box, { target: { value: "/clear" } });
  expect(disabled(sendButton(chat))).toBe(true);
  expect(text(chat)).toContain(note);
  fireEvent.click(sendButton(chat));
  expect(chatCommands()).toEqual([]);
  fireEvent.change(box, { target: { value: "/compact" } });
  expect(disabled(sendButton(chat))).toBe(false);
  expect(text(chat)).not.toContain(note);
});

it("chat composer disables send when the message with attachments exceeds the cap", async () => {
  const { store, chat } = await start("idle");
  // 60,000 characters, 97,000 UTF-8 bytes: under the 100,000-byte cap on its own.
  const value = "é".repeat(37_000) + "a".repeat(23_000);
  fireEvent.change(await composer(chat), { target: { value } });
  expect(disabled(sendButton(chat))).toBe(false);
  // The block keeps 2,000 two-byte characters of the passage, which crosses the cap.
  addDraft(store, draft("doc/product/section_01.md", "é".repeat(50_000)));
  await waitFor(() => expect(disabled(sendButton(chat))).toBe(true));
  expect(text(chat)).toMatch(/over|exceed|too (long|large)/i);
});

it("chat eleventh attachment is refused with a note", async () => {
  const { store, chat } = await start("idle");
  const notes = Array.from({ length: 11 }, (_, n) => `note-${String(n + 1).padStart(2, "0")}`);
  for (const note of notes) addDraft(store, draft(`doc/product/${note}.md`, `Passage ${note}`));
  await waitFor(() => expect(text(chat)).toContain("At most 10 attachments per message"));
  const chips = chipsNamed(chat, /note-\d\d\.md/);
  const more = Number(/\+(\d+) more/.exec(text(chat))?.[1] ?? 0);
  expect(chips.length === 10 || chips.length + more === 10).toBe(true);
  expect(chipsNamed(chat, /note-11\.md/)).toEqual([]);
});

it("chat stop puts queued messages back into the composer", async () => {
  const queue = ["first", "second"].map((line) => ({ id: line, first_line: line, text: line }));
  const { chat } = await start("working", [tool("Read")], { extra: { queue } });
  expect((await composer(chat)).value).toBe("");
  fireEvent.click(within(chat).getByRole("button", { name: /^stop/i }));
  const interrupt = () => chatCommands().find((command) => command.command === "interrupt");
  await waitFor(() => expect(interrupt()).toBeDefined());
  const result = { frame: "result", client_id: interrupt()?.client_id, ok: true, error: null };
  const returned = { type: "chat", frame: "queue_returned", texts: ["first", "second"] };
  seed(returned, { type: "chat", ...result });
  await waitFor(async () => expect((await composer(chat)).value).toBe("first\n\nsecond"));
});

it("chat draft and chips survive a remount through local storage", async () => {
  const first = await start("idle");
  fireEvent.change(await composer(first.chat), { target: { value: "Keep this draft" } });
  addDraft(first.store, draft("doc/product/keep.md", "Kept passage"));
  await waitFor(() => expect(stored()).toContain("Keep this draft"));
  await waitFor(() => expect(stored()).toContain("Kept passage"));
  const { chat } = await start("idle");
  await waitFor(async () => expect((await composer(chat)).value).toBe("Keep this draft"));
  expect(chipsNamed(chat, /keep\.md/)).toHaveLength(1);
});

it("chat turn footer lists a file outside doc without a diff link", async () => {
  const paths = ["doc/product/section_02.md", "planning-omissions.md"];
  const footer = await footerOf(start("idle", [tool("Edit")], { turn: changedFiles(paths) }));
  const links = within(footer).queryAllByRole("link", { name: /diff/i });
  const diffs = [...links, ...within(footer).queryAllByRole("button", { name: /diff/i })];
  expect(diffs).toHaveLength(1);
  expect(text(ancestorWith(diffs[0], "section_02.md"))).not.toContain("planning-omissions.md");
  expect(text(footer)).toContain("planning-omissions.md");
  expect(text(footer)).toContain("outside doc/");
});

it("chat restore all asks first and lists the files", async () => {
  const paths = ["doc/product/section_02.md", "doc/product/section_03.md"];
  replies["/api/restore"] = { files: paths.map((path) => ({ path, result: "restored" })) };
  const footer = await footerOf(start("idle", [tool("Edit")], { turn: changedFiles(paths) }));
  fireEvent.click(within(footer).getByRole("button", { name: /restore all/i }));
  const confirm = () => screen.queryByRole("alertdialog") ?? screen.queryByRole("dialog");
  const dialog = await waitFor(() => confirm() ?? Promise.reject(new Error("no confirmation")));
  expect(posts("/api/restore")).toEqual([]);
  expect(text(dialog)).toContain("section_02.md");
  expect(text(dialog)).toContain("section_03.md");
  fireEvent.click(within(dialog).getByRole("button", { name: /restore/i }));
  await waitFor(() => expect(posts("/api/restore")).toHaveLength(1));
  for (const path of paths) expect(posts("/api/restore")[0]).toContain(path);
});

it("chat panel is mounted on concept and file pages", async () => {
  expect((await start(null, [], { path: "/f/demo/product" })).chat).toBeTruthy();
  expect((await start(null, [], { path: FILE_PAGE })).chat).toBeTruthy();
});

it("chat changed since you looked lists documents whose hash differs from the checkpoint", async () => {
  const hashes = { "doc/product/alpha.md": "h-alpha", "doc/product/beta.md": "h-beta" };
  const checkpoint = JSON.stringify({ commit: "c1", hashes });
  localStorage.setItem("loom-plan:example-1234:checkpoint", checkpoint);
  const docs = { ...hashes, "doc/product/beta.md": "h-beta-2", "doc/product/gamma.md": "h-gamma" };
  replies["/api/hashes"] = { head: "c2", docs };
  await mount("/f/demo/product");
  seed(snapshot(null, []));
  const control = await screen.findByRole("button", { name: /Changed since you looked/ });
  await waitFor(() => expect(text(control)).toMatch(/\b2\b/));
  fireEvent.click(control);
  const [gamma] = await screen.findAllByText(/gamma\.md/);
  const list = text(ancestorWith(gamma, "beta.md"));
  expect(list).not.toContain("alpha.md");
  expect(list).toMatch(/changed/i);
  expect(list).toMatch(/added/i);
});

it("chat permission card shows bidi and zero width characters as visible escapes", async () => {
  const [rlo, zwsp] = [String.fromCodePoint(0x202e), String.fromCodePoint(0x200b)];
  const { chat } = await start("permission", [bashPermission(`ls doc/${rlo}txt.hsab${zwsp}`)]);
  const shown = (await pinnedCard(chat)).textContent ?? "";
  expect(shown).toContain("U+202E");
  expect(shown).toContain("U+200B");
  expect(shown.includes(rlo) || shown.includes(zwsp)).toBe(false);
});

it("chat viewer selection in the file reading column offers send to author", async () => {
  await mount(FILE_PAGE, true);
  seed(snapshot(null, []));
  const words = await within(await screen.findByRole("main")).findByText("Text");
  // jsdom ranges have no layout; plan 3's selection hook measures one to place its buttons.
  const rect = { x: 10, y: 10, width: 40, height: 20, top: 10, left: 10, right: 50, bottom: 30 };
  const measure = { configurable: true, value: () => rect };
  Object.defineProperty(Range.prototype, "getBoundingClientRect", measure);
  act(() => {
    const range = document.createRange();
    range.selectNodeContents(words);
    window.getSelection()?.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  });
  expect(await screen.findByRole("button", { name: /send to author/i })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /add to chat/i })).toBeNull();
});
