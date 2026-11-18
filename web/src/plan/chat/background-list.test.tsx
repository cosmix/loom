import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import backgroundFixture from "@/plan/api/fixtures/background.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import type { BackgroundTaskView } from "@/plan/api/schema-background";
import { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import { backgroundWord, foldBackground } from "./background-fold";
import { installChatSubscription } from "./model";
import { PanelHead } from "./panel-head";

type Obj = Record<string, unknown>;

const TASKS = backgroundFixture.tasks as BackgroundTaskView[];
const [RUNNING, SHELL, QUIET, DONE, FAILED_SHELL] = TASKS as [
  BackgroundTaskView,
  BackgroundTaskView,
  BackgroundTaskView,
  BackgroundTaskView,
  BackgroundTaskView,
];
const RLO = String.fromCodePoint(0x202e);

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

function hello(): Obj {
  const base = structuredClone(helloFixture);
  const holder = { ...base.you, away_since: null, frees_at: null };
  return { ...base, server: { ...base.server, read_only: [] }, authoring: { holder } };
}

/** A chat snapshot frame of the fixture's session, idle and held by the viewer. */
function idleChat(): Obj {
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

const background = (tasks: BackgroundTaskView[]): Obj => ({
  ...structuredClone(backgroundFixture),
  tasks: structuredClone(tasks),
});

const deliver = (...frames: Obj[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

const text = (element: Element | null | undefined): string =>
  (element?.textContent ?? "").replace(/\s+/g, " ").trim();

/** The panel head of an idle chat fed `tasks` by the socket, its list opened. */
async function openList(tasks: BackgroundTaskView[], prepare = (): void => {}): Promise<Element> {
  const store = createStore();
  initClient(store);
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  store.set(planSocketAtom, handle);
  disposers.push(handle.dispose, installChatSubscription(store));
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PanelHead sheet={false} />
      </MemoryRouter>
    </Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => FakeSocket.last?.open());
  deliver(hello(), idleChat(), background(tasks));
  prepare();
  const disclosure = screen.getByRole("button", { name: "Background tasks" });
  fireEvent.click(disclosure);
  return document.getElementById(disclosure.getAttribute("aria-controls")!)!;
}

/** The row whose label is `label`. */
function row(list: Element, label: string): Element {
  const found = Array.from(list.querySelectorAll("li")).find(
    (item) => text(item.querySelector(".chat-background-label")) === label,
  );
  if (!found) throw new Error(`no row labelled ${label}`);
  return found;
}

const part = (item: Element, name: string): string =>
  text(item.querySelector(`.chat-background-${name}`));

const shell = (id: string, state: BackgroundTaskView["state"], label: string) => ({
  ...SHELL,
  id,
  state,
  label,
});

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (input) => {
      const { pathname } = new URL(String(input), "http://localhost");
      const body = { ...helloFixture.server, read_only: [] };
      return pathname === "/api/server"
        ? new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } })
        : new Response(JSON.stringify({ error: "not_found" }), { status: 404 });
    }),
  );
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  localStorage.clear();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("every state has its word, and tool-wait reads as running", () => {
  const words = (["running", "tool-wait", "stalled", "done", "failed", "stopped"] as const).map(
    backgroundWord,
  );
  expect(words).toEqual(["running", "running", "stalled?", "done", "failed", "stopped"]);
});

it("the fold keeps unended tasks and ended shells and counts ended agents", () => {
  const failedAgent = { ...DONE, id: "a-failed", state: "failed" as const };
  const folded = foldBackground([DONE, RUNNING, FAILED_SHELL, failedAgent, QUIET]);
  expect(folded).toEqual([
    { kind: "task", task: RUNNING },
    { kind: "task", task: FAILED_SHELL },
    { kind: "task", task: QUIET },
    { kind: "earlier", count: 2 },
  ]);
  expect(foldBackground([RUNNING, SHELL])).toEqual([
    { kind: "task", task: RUNNING },
    { kind: "task", task: SHELL },
  ]);
  expect(foldBackground([])).toEqual([]);
});

it("ended agents fold into one earlier agents line under the rows", async () => {
  const stopped = { ...DONE, id: "a-stopped", state: "stopped" as const, label: "Map the hooks" };
  const list = await openList([...TASKS, stopped]);

  const rows = Array.from(list.querySelectorAll("li"));
  expect(rows.map((item) => part(item, "label"))).toEqual([
    RUNNING.label,
    SHELL.label,
    QUIET.label,
    FAILED_SHELL.label,
    "",
  ]);
  expect(text(rows.at(-1))).toBe("2 earlier agents");
  expect(list.textContent).not.toContain(DONE.label);
  expect(list.textContent).not.toContain("Map the hooks");

  // One ended agent is counted in the singular; none leaves no line.
  deliver(background(TASKS));
  expect(text(list.querySelector("li:last-child"))).toBe("1 earlier agent");
  deliver(background([RUNNING, SHELL]));
  expect(list.textContent).not.toContain("earlier");
});

it("each row leads with one mark and its state word", async () => {
  const list = await openList([
    RUNNING,
    QUIET,
    shell("s-stopped", "stopped", "Watch the build"),
    shell("s-failed", "failed", "Run the failing check"),
    shell("s-done", "done", "Fetch the schema"),
    { ...RUNNING, id: "a-wait", state: "tool-wait", label: "Trace the relay" },
  ]);

  const states = Array.from(list.querySelectorAll("li")).map((item) => [
    part(item, "label"),
    part(item, "state"),
    item.querySelectorAll(".chat-background-glyph svg").length,
  ]);
  expect(states).toEqual([
    [RUNNING.label, "running", 1],
    [QUIET.label, "stalled?", 1],
    ["Watch the build", "stopped", 1],
    ["Run the failing check", "failed", 1],
    ["Fetch the schema", "done", 1],
    ["Trace the relay", "running", 1],
  ]);

  // Work under way turns the arrows; a quiet task rests them; an ended one has none.
  const arrows = (label: string) =>
    row(list, label).querySelector(".plan-arrow-spinner")?.getAttribute("data-busy") ?? null;
  expect(arrows(RUNNING.label!)).toBe("true");
  expect(arrows("Trace the relay")).toBe("true");
  expect(arrows(QUIET.label!)).toBe("false");
  expect(arrows("Watch the build")).toBeNull();
  expect(arrows("Run the failing check")).toBeNull();
  // The marks are decoration: the word beside each is what is read.
  const hidden = Array.from(list.querySelectorAll(".chat-background-glyph")).map((glyph) =>
    glyph.getAttribute("aria-hidden"),
  );
  expect(hidden).toEqual(Array<string>(6).fill("true"));
});

it("every row's elapsed time comes from the list's one clock", async () => {
  // Spied once the fake timers stand in, so the spy sees the list's own call.
  const list = await openList(TASKS, () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-09T10:05:00Z"));
    vi.spyOn(window, "setInterval");
  });

  const clocks = () =>
    [RUNNING, SHELL, QUIET, FAILED_SHELL].map((task) => part(row(list, task.label!), "clock"));
  // The failed shell shows how long it ran, not how long ago it started.
  expect(clocks()).toEqual(["5m0s", "3m50s", "3m0s", "20s"]);
  expect(vi.mocked(window.setInterval).mock.calls.map((call) => call[1])).toEqual([1000]);

  act(() => vi.advanceTimersByTime(1000));
  expect(clocks()).toEqual(["5m1s", "3m51s", "3m1s", "20s"]);

  // Closed, the list keeps no clock running.
  const clears = vi.spyOn(window, "clearInterval");
  fireEvent.click(screen.getByRole("button", { name: "Background tasks" }));
  expect(clears).toHaveBeenCalledTimes(1);
});

it("an agent row adds its model and tokens, and a figure that is not set draws nothing", async () => {
  const fresh = { ...RUNNING, id: "a-fresh", label: "Read the brief", model: null, total: null };
  const list = await openList([RUNNING, SHELL, QUIET, { ...fresh, tokens_per_min: null }]);

  expect(part(row(list, RUNNING.label!), "figures")).toBe(
    "claude-sonnet-5-5 · 48.2k tokens · 3.1k/m",
  );
  // A stalled agent's rate of zero is left out; its total stands.
  expect(part(row(list, QUIET.label!), "figures")).toBe("claude-haiku-5 · 9.1k tokens");
  for (const label of [SHELL.label!, "Read the brief"]) {
    expect(row(list, label).querySelector(".chat-background-figures")).toBeNull();
  }
  expect(list.textContent).not.toMatch(/\b0 tokens|\b0\/m|null|NaN/);
});

it("a row with no label is named by its agent type, else its kind, through visible marks", async () => {
  const list = await openList([
    { ...RUNNING, id: "a-typed", label: null, agent_type: `Ex${RLO}plore`, model: `son${RLO}net` },
    { ...RUNNING, id: "a-bare", label: " ", agent_type: null },
    { ...SHELL, id: "s-bare", label: null },
  ]);

  const labels = Array.from(list.querySelectorAll(".chat-background-label")).map(text);
  expect(labels).toEqual(["Ex⟨U+202E⟩plore", "Agent", "Shell"]);
  expect(list.querySelector(".chat-background-label")!.getAttribute("title")).toBe(
    "Ex⟨U+202E⟩plore",
  );
  expect(list.textContent).toContain("son⟨U+202E⟩net");
  expect(document.body.textContent).not.toContain(RLO);
});
