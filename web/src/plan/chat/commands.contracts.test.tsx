import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import sessionCommands from "@/plan/api/fixtures/session-commands.json";
import slashVectors from "@/plan/api/fixtures/slash-vectors.json";
import type { Attachment } from "@/plan/api/schema-chat";
import type { CommandsView } from "@/plan/api/schema-session-commands";
import { connectPlanSocket } from "@/plan/api/socket";
import { activeFeatureAtom, planSocketAtom } from "@/plan/state/atoms";
import * as gate from "./message-problem";
import { ChatPanel } from "./panel";

type Obj = Record<string, unknown>;
type Vector = { text: string; table: string; expect: string };
// The stage adds the commands argument; the cast keeps this file loading before it exists.
type Gate = (text: string, attachments: Attachment[], commands: CommandsView) => string | null;

const SESSION_ID = snapshotFixture.session.id;

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

const deliver = (...frames: Obj[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

/** Time for anything a key press set off to reach the socket. */
const settle = (): Promise<void> =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });

const chatCommands = (): Obj[] => (FakeSocket.last?.sent ?? []).filter((s) => s.type === "chat");

/** Renders the chat panel, opens the socket, and delivers hello, the idle chat and its commands. */
async function mountHolderComposer(): Promise<HTMLTextAreaElement> {
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
  const commands = { type: "commands", session: SESSION_ID, ...structuredClone(sessionCommands) };
  deliver(hello(), idleSnapshot(), commands);
  const chat = await screen.findByRole("complementary", { name: "Chat" });
  return within(chat).findByLabelText("Message", { selector: "textarea" });
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
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (input) => {
      const path = new URL(String(input), "http://localhost").pathname;
      if (path === "/api/server") return json({ ...helloFixture.server, read_only: [] });
      if (path === "/api/session/commands") return json(sessionCommands);
      return json({ error: "not_found", message: "not found" }, 404);
    }),
  );
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("slash vectors get the server answers in the composer gate", () => {
  const messageProblem = gate.messageProblem as unknown as Gate;
  const tables = slashVectors.tables as Record<string, CommandsView>;
  const cases = slashVectors.cases as Vector[];
  expect(cases.length).toBeGreaterThan(0);
  const disagreements = cases.flatMap((vector) => {
    const table = tables[vector.table];
    if (table === undefined) return [{ ...vector, got: "no such table" }];
    const got = messageProblem(vector.text, [], table);
    const wanted = vector.expect === "ok" ? null : vector.expect;
    return got === wanted ? [] : [{ ...vector, got }];
  });
  expect(disagreements).toEqual([]);
});

it("typing a slash opens the command list and keeps focus", async () => {
  const box = await mountHolderComposer();
  act(() => box.focus());
  fireEvent.change(box, { target: { value: "/" } });

  const list = await screen.findByRole("listbox");
  expect(box.getAttribute("role")).toBe("combobox");
  expect(box.getAttribute("aria-expanded")).toBe("true");
  expect(list.id).not.toBe("");
  expect(box.getAttribute("aria-controls")).toBe(list.id);
  await waitFor(() => expect(document.activeElement).toBe(box));

  // The first entry is highlighted on open; ArrowDown moves to the second and Enter inserts it.
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter" });
  const second = sessionCommands.entries[1]!.name;
  await waitFor(() => expect(box.value).toBe(`/${second} `));
  await settle();
  expect(chatCommands()).toEqual([]);
  expect(document.activeElement).toBe(box);

  // A fresh slash opens the list again, and Escape closes it.
  fireEvent.change(box, { target: { value: "/" } });
  await screen.findByRole("listbox");
  fireEvent.keyDown(box, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  expect(box.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(box);
  expect(chatCommands()).toEqual([]);
});
