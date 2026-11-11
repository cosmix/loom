import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import sessionCommands from "@/plan/api/fixtures/session-commands.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { activeFeatureAtom, planSocketAtom } from "@/plan/state/atoms";
import { CommandChip } from "./command-chip";
import { ChatPanel } from "./panel";

type Obj = Record<string, unknown>;

const RLO = String.fromCodePoint(0x202e);
const COMPACT_HINT = "<optional custom summarization instructions>";

// The plan socket's test seam: connectPlanSocket with a WebSocket stand-in.
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

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function hello(): Obj {
  const base = structuredClone(helloFixture);
  const holder = { ...base.you, away_since: null, frees_at: null };
  return { ...base, server: { ...base.server, read_only: [] }, authoring: { holder } };
}

/** A chat snapshot frame of the fixture's session, idle and held by the viewer. */
function idleSnapshot(): Obj {
  const base = structuredClone(snapshotFixture);
  const session = { ...base.session, state: "idle", holder: helloFixture.you.name, queued: 0 };
  return {
    ...base,
    type: "chat",
    frame: "snapshot",
    session,
    turns: [],
    queue: [],
    previous: null,
  };
}

const field = () => screen.getByLabelText<HTMLTextAreaElement>("Message");
const write = (text: string) => fireEvent.change(field(), { target: { value: text } });
const press = (key: string) => fireEvent.keyDown(field(), { key });
const send = () => screen.getByRole("button", { name: "Send" });
const sent = (): Obj[] => (FakeSocket.last?.sent ?? []).filter((frame) => frame.type === "chat");
const options = () => screen.queryAllByRole("option");
const named = (option: Element) => option.querySelector(".chat-command-name")?.textContent;
const shown = () => options().map(named);
const option = (name: string) => options().find((entry) => named(entry) === name)!;
const text = (element: Element) => (element.textContent ?? "").replace(/\s+/g, " ").trim();

/** The holder's composer of an idle chat whose session lists `commands`, sent as its frame. */
async function mountComposer(commands: Obj = sessionCommands): Promise<void> {
  const store = createStore();
  initClient(store);
  store.set(activeFeatureAtom, "demo");
  FakeSocket.last = undefined;
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  disposers.push(handle.dispose);
  store.set(planSocketAtom, handle);
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <ChatPanel /> }], {
    initialEntries: ["/f/demo/product"],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  const frame = { type: "commands", session: snapshotFixture.session.id, ...commands };
  act(() => {
    FakeSocket.last?.open();
    [hello(), idleSnapshot(), frame].forEach((each) => FakeSocket.last?.receive(each));
  });
  await screen.findByLabelText("Message");
}

/** Mounts the composer, focuses its text box and types `typed`, which opens the list. */
async function open(typed = "/", commands: Obj = sessionCommands): Promise<void> {
  await mountComposer(commands);
  act(() => field().focus());
  write(typed);
  await screen.findByRole("listbox", { name: "Commands" });
}

beforeEach(() => {
  // Every min-width breakpoint holds, so the desktop panel and its composer mount.
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: /min-width/.test(query) && !/max-width|prefers-reduced-motion/.test(query),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
  // The commands route answers nothing useful: the list under test is the socket's frame.
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (input) =>
      new URL(String(input), "http://localhost").pathname === "/api/server"
        ? json({ ...helloFixture.server, read_only: [] })
        : json({ error: "not_found", message: "not found" }, 404),
    ),
  );
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("lists every entry for a bare slash and filters by the typed name", async () => {
  await open();
  expect(shown()).toEqual(sessionCommands.entries.map((entry) => `/${entry.name}`));
  expect(options()[0]!.getAttribute("aria-selected")).toBe("true");

  write("/co");
  expect(shown()).toEqual(["/context", "/compact", "/usage"]);
  // An alias finds its command, and the highlight returns to the first match.
  write("/arg");
  expect(shown()).toEqual(["/loom-plan:args"]);
  expect(field().getAttribute("aria-activedescendant")).toBe(options()[0]!.id);

  write("/zzz");
  expect(options()).toEqual([]);
  expect(text(screen.getByText(/No command matches/))).toBe("No command matches /zzz");
  // Arguments have begun: the list is gone and the box is a plain text box again.
  write("/compact keep the plan");
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(field().getAttribute("role")).toBeNull();
  expect(field().getAttribute("aria-expanded")).toBeNull();
});

it("moves the highlight with the arrows and inserts with Enter and with Tab", async () => {
  await open();
  press("ArrowUp");
  expect(field().getAttribute("aria-activedescendant")).toBe(options()[0]!.id);
  press("ArrowDown");
  press("ArrowDown");
  expect(options()[2]!.getAttribute("aria-selected")).toBe("true");
  expect(field().getAttribute("aria-activedescendant")).toBe(options()[2]!.id);
  press("Enter");
  expect(field().value).toBe("/probe-personal ");
  expect(screen.queryByRole("listbox")).toBeNull();

  write("/con");
  expect(fireEvent.keyDown(field(), { key: "Tab" })).toBe(false);
  expect(field().value).toBe("/context ");
  await waitFor(() => expect(document.activeElement).toBe(field()));
  expect(sent()).toEqual([]);

  // Past the last entry the highlight stays on it.
  write("/co");
  for (let i = 0; i < 5; i++) press("ArrowDown");
  const selected = options().map((entry) => entry.getAttribute("aria-selected"));
  expect(selected).toEqual(["false", "false", "true"]);
});

it("closes on Escape without letting the key reach the page, and on a press outside", async () => {
  await open();
  const seen = vi.fn();
  document.addEventListener("keydown", seen);
  press("Escape");
  document.removeEventListener("keydown", seen);
  expect(seen).not.toHaveBeenCalled();
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(field().getAttribute("aria-expanded")).toBe("false");
  expect(field().value).toBe("/");

  // The next edit opens it again; a press on the page closes it, a press in the box does not.
  write("/c");
  expect(screen.getByRole("listbox")).toBeTruthy();
  fireEvent.pointerDown(field());
  expect(screen.getByRole("listbox")).toBeTruthy();
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("a Terminal only entry shows its reason and cannot be inserted", async () => {
  await open("/cl");
  const clear = option("/clear");
  expect(clear.getAttribute("aria-disabled")).toBe("true");
  expect(clear.hasAttribute("disabled")).toBe(false);
  expect(clear.getAttribute("aria-selected")).toBe("true");
  expect(text(clear)).toContain("Terminal only · Changes the session outside the planner");

  press("Enter");
  fireEvent.click(clear);
  expect(field().value).toBe("/cl");
  expect(sent()).toEqual([]);
  expect(screen.getByRole("listbox")).toBeTruthy();
  // Tab has nothing to insert: it closes the list and is left to move the focus on.
  expect(fireEvent.keyDown(field(), { key: "Tab" })).toBe(true);
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(field().value).toBe("/cl");

  // An entry that runs carries no such mark.
  write("/sim");
  expect(option("/simplify").getAttribute("aria-disabled")).toBeNull();
  expect(text(option("/simplify"))).not.toContain("Terminal only");
});

it("each entry shows its source word, its description and its argument hint", async () => {
  const [bundled, plugin, ...rest] = sessionCommands.entries;
  const marked = { ...plugin!, description: `Echo${RLO}back` };
  const skill = { ...rest[0]!, name: "probe-skill", source: "skill" };
  await open("/", { ...sessionCommands, entries: [bundled, marked, skill, ...rest] });
  const sources = options().map((entry) => entry.querySelector(".chat-command-source"));
  expect(new Set(sources.map((source) => source?.textContent))).toEqual(
    new Set(["built-in", "bundled", "plugin", "personal", "account", "skill"]),
  );
  expect(text(option("/compact"))).toBe(
    `/compact ${COMPACT_HINT} built-in Free up context by summarizing the conversation so far`,
  );
  // Server text shows its control characters as visible marks.
  expect(text(option("/loom-plan:args"))).toContain("Echo⟨U+202E⟩back");
  expect(document.body.textContent).not.toContain(RLO);
});

it("the Commands button opens the same list and a pick keeps the draft as arguments", async () => {
  await mountComposer();
  write("tidy the gate");
  const button = screen.getByRole("button", { name: "Commands" });
  expect(button.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(button);
  const list = await screen.findByRole("listbox", { name: "Commands" });
  expect(options()).toHaveLength(sessionCommands.entries.length);
  expect(button.getAttribute("aria-expanded")).toBe("true");
  expect(button.getAttribute("aria-controls")).toBe(list.id);
  expect(field().getAttribute("role")).toBe("combobox");
  expect(field().getAttribute("aria-controls")).toBe(list.id);
  await waitFor(() => expect(document.activeElement).toBe(field()));

  fireEvent.click(option("/simplify"));
  expect(field().value).toBe("/simplify tidy the gate");
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(sent()).toEqual([]);

  // A second press of the button closes the list it opened.
  fireEvent.click(button);
  expect(screen.getByRole("listbox")).toBeTruthy();
  fireEvent.click(button);
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("a picked command leaves its argument hint where the arguments go", async () => {
  await open("/comp");
  press("Enter");
  expect(field().value).toBe("/compact ");
  const hint = document.querySelector<HTMLElement>(".chat-command-ghost")!;
  expect(hint.textContent).toBe(`/compact ${COMPACT_HINT}`);
  expect(within(hint).getByText(COMPACT_HINT)).toBeTruthy();
  expect(field().getAttribute("aria-describedby")).toContain(hint.id);
  expect(field().placeholder).toBe(COMPACT_HINT);

  write("/compact keep the plan");
  expect(document.querySelector(".chat-command-ghost")).toBeNull();
  expect(field().placeholder).not.toBe(COMPACT_HINT);
});

it("Send emits the literal command line", async () => {
  await open("/arg");
  press("Enter");
  expect(field().value).toBe("/loom-plan:args ");
  write("/loom-plan:args alpha beta");
  expect(send().getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(send());
  await waitFor(() => expect(sent()).toHaveLength(1));
  const line = { command: "send", text: "/loom-plan:args alpha beta", attachments: [] };
  expect(sent()[0]).toMatchObject(line);
});

it("a sent command is one chip row of its name and arguments, controls made visible", () => {
  const { container } = render(
    <CommandChip command={{ name: "loom-plan:args", args: `alpha${RLO}beta` }} />,
  );
  const chip = container.querySelector<HTMLElement>('.chat-chip[data-kind="command"]')!;
  expect(text(chip)).toBe("/loom-plan:args alpha⟨U+202E⟩beta");
  expect(chip.title).toBe("/loom-plan:args alpha⟨U+202E⟩beta");
  expect(container.textContent).not.toContain(RLO);

  cleanup();
  const bare = render(<CommandChip command={{ name: "simplify", args: "" }} />).container;
  expect(text(bare)).toBe("/simplify");
  expect(bare.querySelector(".chat-command-chip-args")).toBeNull();
});

it("a list that is still loading says so and holds a listed command back", async () => {
  const native = sessionCommands.entries.filter((entry) => entry.source === "built-in");
  await open("/", { state: "loading", generation: null, reason: null, entries: native });
  expect(options()).toHaveLength(native.length);
  const status = screen.getByText("Loading this session's commands").closest("p")!;
  expect(screen.getByRole("listbox").getAttribute("aria-describedby")).toBe(status.id);

  write("/simplify");
  const blocked = { name: "Send", description: "Commands are still loading" };
  expect(screen.getByRole("button", blocked)).toBeTruthy();
  write("/compact");
  expect(send().getAttribute("aria-disabled")).toBeNull();
});
