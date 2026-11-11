import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import backgroundFixture from "@/plan/api/fixtures/background.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { activeFeatureAtom, planSocketAtom } from "@/plan/state/atoms";
import { ChatPanel } from "./panel";

type Obj = Record<string, unknown>;

const SESSION_ID = snapshotFixture.session.id;
const OTHER_SESSION = "00000000-0000-4000-8000-0000000000ff";
const UNENDED = backgroundFixture.tasks.filter((task) =>
  ["running", "stalled"].includes(task.state),
);
const RLO = String.fromCodePoint(0x202e);
const AGENT_SUMMARY = "Survey the session bridge";

// The plan socket's test seam: connectPlanSocket with a WebSocket stand-in.
class FakeSocket {
  static last: FakeSocket | undefined;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  constructor() {
    FakeSocket.last = this;
  }
  send = (): void => {};
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

function hello(): Obj {
  const base = structuredClone(helloFixture);
  const holder = { ...base.you, away_since: null, frees_at: null };
  return { ...base, server: { ...base.server, read_only: [] }, authoring: { holder } };
}

/** A chat snapshot frame of the fixture's session, idle and held by the viewer. */
function idleSnapshot(turns: Obj[] = []): Obj {
  const base = structuredClone(snapshotFixture);
  const session = { ...base.session, state: "idle", holder: helloFixture.you.name, queued: 0 };
  return { ...base, type: "chat", frame: "snapshot", session, turns, queue: [], previous: null };
}

/** A background frame of the fixture, its tasks replaced when `tasks` is given. */
function backgroundFrame(session: string, tasks: Obj[] = backgroundFixture.tasks): Obj {
  return { ...structuredClone(backgroundFixture), session, tasks: structuredClone(tasks) };
}

const deliver = (...frames: Obj[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

/** Time for anything a frame set off to render. */
const settle = (): Promise<void> =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });

const text = (element: Element): string => (element.textContent ?? "").replace(/\s+/g, " ").trim();

/** Renders the chat panel on a feature route, opens the socket and delivers hello and `chat`. */
async function mountChat(chat: Obj): Promise<HTMLElement> {
  const store = createStore();
  initClient(store);
  store.set(activeFeatureAtom, "demo");
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
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
  act(() => FakeSocket.last?.open());
  deliver(hello(), chat);
  return screen.findByRole("complementary", { name: "Chat" });
}

/** The span inside `.chat-head-sub` that reads "Waiting on ...", or null. */
function backgroundLine(): string | null {
  const sub = document.querySelector(".chat-head-sub");
  const spans = Array.from(sub?.querySelectorAll("span") ?? []);
  const line = spans.find((span) => /^Waiting on \d+ background tasks?$/.test(text(span)));
  return line ? text(line) : null;
}

function toolItem(id: string, fields: Obj): Obj {
  const base = snapshotFixture.turns[0]!.items.find((entry) => entry.type === "tool")!;
  return { ...structuredClone(base), id, status: "done", ...fields };
}

/** A closed turn: two done Read rows, then the done launch row of the fixture's running agent. */
function closedTurnWithAgent(): Obj {
  const read = (n: number) =>
    toolItem(`read-${n}`, { tool_use_id: `toolu_read${n}`, summary: `Read doc/plans/p${n}.md` });
  const agent = toolItem("agent-1", {
    tool_use_id: backgroundFixture.tasks[0]!.tool_use_id,
    name: "Agent",
    summary: AGENT_SUMMARY,
    input: JSON.stringify({ description: AGENT_SUMMARY, run_in_background: true }),
    result: "Async agent launched",
  });
  const base = structuredClone(snapshotFixture.turns[0]!);
  return { ...base, id: "t1", open: false, changed_files: [], items: [read(1), read(2), agent] };
}

/** The tool head button of the Agent row; the row is the button's parent. */
const agentHead = (): HTMLElement =>
  screen.getByRole("button", { name: new RegExp(AGENT_SUMMARY), hidden: true });

beforeEach(() => {
  // Every min-width breakpoint holds, so the desktop panel and its head mount.
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

it("idle chat shows the background line from a socket frame", async () => {
  await mountChat(idleSnapshot());
  expect(UNENDED.length).toBe(3);
  deliver(backgroundFrame(SESSION_ID));

  const expected = `Waiting on ${UNENDED.length} background tasks`;
  await waitFor(() => expect(backgroundLine()).toBe(expected));
  expect(screen.getByRole("button", { name: "Background tasks" })).toBeTruthy();

  // Another session's tasks never reach this chat's line.
  deliver(backgroundFrame(OTHER_SESSION, [backgroundFixture.tasks[0]!]));
  await settle();
  expect(backgroundLine()).toBe(expected);
});

it("background list rows show label controls as visible marks", async () => {
  await mountChat(idleSnapshot());
  const tasks = structuredClone(backgroundFixture.tasks);
  tasks[0] = { ...tasks[0]!, label: `run${RLO}evil` };
  deliver(backgroundFrame(SESSION_ID, tasks));

  const disclosure = await screen.findByRole("button", { name: "Background tasks" });
  fireEvent.click(disclosure);

  const listId = disclosure.getAttribute("aria-controls");
  expect(listId).toBeTruthy();
  await waitFor(() => expect(document.getElementById(listId!)).not.toBeNull());
  const list = document.getElementById(listId!)!;
  expect(list.closest(".chat-head-sub")).toBeNull();
  await waitFor(() => expect(list.textContent).toContain("run⟨U+202E⟩evil"));
  expect(list.textContent).not.toContain(RLO);
  expect(document.body.textContent).not.toContain(RLO);
});

it("a running background row stays outside the tool fold", async () => {
  await mountChat(idleSnapshot([closedTurnWithAgent()]));
  const running = backgroundFixture.tasks[0]!;
  expect(running.state).toBe("running");
  deliver(backgroundFrame(SESSION_ID, [running]));

  await waitFor(() => expect(agentHead().closest("details.chat-tool-fold")).toBeNull());
  const row = agentHead().parentElement!;
  expect(text(row)).toMatch(/\brunning\b/i);
  expect(text(row)).not.toMatch(/\bdone\b/i);
  // The two plain Read rows still fold.
  expect(document.querySelectorAll("details.chat-tool-fold").length).toBeGreaterThan(0);

  // Once the task ends, its launch row is a plain finished tool row and folds with the rest.
  deliver(backgroundFrame(SESSION_ID, [{ ...running, state: "done", report_preview: "DONE" }]));
  await waitFor(() => expect(agentHead().closest("details.chat-tool-fold")).not.toBeNull());
});
