import * as Dialog from "@radix-ui/react-dialog";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { useState, type ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import backgroundFixture from "@/plan/api/fixtures/background.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import { installChatSubscription } from "./model";
import { PanelHead } from "./panel-head";

type Obj = Record<string, unknown>;

const SESSION_ID = snapshotFixture.session.id;
const TASKS = backgroundFixture.tasks;
const ENDED = TASKS.filter((task) => ["done", "failed", "stopped"].includes(task.state));
const HOLD_MS = 7000;

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

/** A chat snapshot frame of the fixture's session in `state`, held by the viewer. */
function chat(state: string, fields: Obj = {}): Obj {
  const base = structuredClone(snapshotFixture);
  const session = { ...base.session, state, holder: helloFixture.you.name, queued: 0, ...fields };
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

/** A background frame of the fixture's session holding `tasks`. */
function background(tasks: Obj[] = TASKS, session = SESSION_ID): Obj {
  return { ...structuredClone(backgroundFixture), session, tasks: structuredClone(tasks) };
}

const deliver = (...frames: Obj[]): void =>
  act(() => frames.forEach((frame) => FakeSocket.last?.receive(frame)));

const text = (element: Element | null): string =>
  (element?.textContent ?? "").replace(/\s+/g, " ").trim();

/** The sheet below 1280px: the head inside a Radix dialog, which closes on Escape. */
function Sheet({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Content aria-describedby={undefined}>
          <Dialog.Title>Chat</Dialog.Title>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * The panel head on a store fed by the socket: opens it and delivers hello and `first`. With
 * `inSheet` it is the sheet's head, inside the dialog the real sheet mounts it in.
 */
async function mountHead(first: Obj, inSheet = false): Promise<HTMLElement> {
  const store = createStore();
  initClient(store);
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  store.set(planSocketAtom, handle);
  disposers.push(handle.dispose, installChatSubscription(store));
  const head = (
    <MemoryRouter>
      <PanelHead sheet={inSheet} />
    </MemoryRouter>
  );
  const { container } = render(
    <Provider store={store}>{inSheet ? <Sheet>{head}</Sheet> : head}</Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => FakeSocket.last?.open());
  deliver(hello(), first);
  return container;
}

const line = (head: HTMLElement): string | null => {
  const found = head.querySelector(".chat-head-sub .chat-background-line");
  return found ? text(found) : null;
};

const disclosure = (): HTMLElement | null =>
  screen.queryByRole("button", { name: "Background tasks" });

/** What the status regions say about background tasks; the state line's own words are left out. */
const announced = (): string[] =>
  screen
    .queryAllByRole("status")
    .map(text)
    .filter((words) => /background/i.test(words));

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

it("an idle chat with no task draws no element and no status region", async () => {
  const head = await mountHead(chat("idle"));
  const regions = screen.queryAllByRole("status").length;
  // The title and the sub-line, and nothing after them.
  expect(head.querySelector(".chat-head-text")!.children.length).toBe(2);
  expect(text(head.querySelector(".chat-head-sub"))).toBe("");
  expect(head.querySelector(".chat-background-line")).toBeNull();
  expect(disclosure()).toBeNull();

  // A table for this session with nothing in it changes nothing.
  deliver(background([]));
  expect(head.querySelector(".chat-head-text")!.children.length).toBe(2);
  expect(screen.queryAllByRole("status").length).toBe(regions);
});

it("a socket frame gives an idle chat the line and its disclosure", async () => {
  const head = await mountHead(chat("idle", { feature: "demo" }));
  deliver(background());

  expect(line(head)).toBe("Waiting on 3 background tasks");
  // The line is not a state row, and it joins the place with the state words' separator.
  expect(head.querySelector(".chat-state-row")).toBeNull();
  expect(text(head.querySelector(".chat-head-sub"))).toBe("Waiting on 3 background tasks · demo");
  const button = disclosure()!;
  expect(button.getAttribute("aria-expanded")).toBe("false");
  // The list is not rendered, so the disclosure controls nothing yet.
  expect(button.hasAttribute("aria-controls")).toBe(false);
  expect(button.closest(".chat-background-line")).not.toBeNull();

  deliver(background([TASKS[0]!]));
  expect(line(head)).toBe("Waiting on 1 background task");

  // Another session's tasks never reach this chat's line.
  deliver(background(TASKS, "00000000-0000-4000-8000-0000000000ff"));
  expect(line(head)).toBe("Waiting on 1 background task");
});

it("a working chat with tasks shows its state words and no line", async () => {
  const head = await mountHead(chat("working"));
  deliver(background());

  expect(text(head.querySelector(".chat-state-row"))).toBe("Working");
  expect(line(head)).toBeNull();
  expect(disclosure()).toBeNull();
  expect(announced()).toEqual([]);
});

it("an idle chat whose tasks have all ended shows nothing", async () => {
  const head = await mountHead(chat("idle"));
  const regions = screen.queryAllByRole("status").length;
  deliver(background(ENDED));

  expect(ENDED.length).toBe(2);
  expect(line(head)).toBeNull();
  expect(disclosure()).toBeNull();
  expect(head.querySelector(".chat-head-text")!.children.length).toBe(2);
  expect(screen.queryAllByRole("status").length).toBe(regions);
});

it("the line is announced once and the last task's end once", async () => {
  const head = await mountHead(chat("working"));
  vi.useFakeTimers();
  deliver(background());
  // The region is in place, empty, before it has words to say.
  const regions = screen.queryAllByRole("status").length;
  expect(announced()).toEqual([]);

  deliver(chat("idle"));
  expect(announced()).toEqual(["Waiting on 3 background tasks"]);
  expect(screen.queryAllByRole("status").length).toBe(regions);

  // One task ends: the line counts two, and the region does not speak again.
  deliver(background(TASKS.slice(1)));
  expect(line(head)).toBe("Waiting on 2 background tasks");
  expect(announced()).toEqual(["Waiting on 3 background tasks"]);

  // The words leave the region once they have been read out.
  act(() => vi.advanceTimersByTime(HOLD_MS));
  expect(announced()).toEqual([]);

  // A turn of work and back to idle: the line returns unannounced.
  deliver(chat("working"));
  deliver(chat("idle"));
  expect(line(head)).toBe("Waiting on 2 background tasks");
  expect(announced()).toEqual([]);

  deliver(background(ENDED));
  expect(line(head)).toBeNull();
  expect(announced()).toEqual(["Background tasks finished"]);

  // Said once: the region goes with its words.
  act(() => vi.advanceTimersByTime(HOLD_MS));
  expect(announced()).toEqual([]);
  expect(screen.queryAllByRole("status").length).toBe(regions - 1);
});

it("an idle chat's first frame mounts the status region empty and gives it words afterwards", async () => {
  await mountHead(chat("idle"));
  const observer = new MutationObserver(() => {});
  observer.observe(document.body, { childList: true, subtree: true });
  deliver(background());
  const records = observer.takeRecords();
  observer.disconnect();

  const region = (node: Node): node is HTMLElement =>
    node instanceof HTMLElement && node.matches('[role="status"].sr-only');
  const inserted = records.findIndex((record) => [...record.addedNodes].some(region));
  expect(inserted).toBeGreaterThanOrEqual(0);
  const added = [...records[inserted]!.addedNodes].find(region)!;
  // A live region inserted already holding its words is not announced: the words come after.
  const words = records.findIndex(
    (record, index) => index > inserted && record.target === added && record.addedNodes.length > 0,
  );
  expect(words).toBeGreaterThan(inserted);
  expect(text(added)).toBe("Waiting on 3 background tasks");
});

it("a reconnect neither ends the wait aloud nor announces the line again", async () => {
  const head = await mountHead(chat("idle"));
  deliver(background());
  expect(announced()).toEqual(["Waiting on 3 background tasks"]);

  // Hello resets the table: no frame yet is not "finished".
  deliver(hello());
  expect(line(head)).toBeNull();
  expect(announced()).toEqual(["Waiting on 3 background tasks"]);

  deliver(background());
  expect(line(head)).toBe("Waiting on 3 background tasks");
  expect(announced()).toEqual(["Waiting on 3 background tasks"]);
});

it("the disclosure opens the list under the sub-line and Escape closes it, focus kept", async () => {
  const head = await mountHead(chat("idle"));
  deliver(background());
  const button = disclosure()!;
  button.focus();

  fireEvent.click(button);
  expect(button.getAttribute("aria-expanded")).toBe("true");
  const list = document.getElementById(button.getAttribute("aria-controls")!)!;
  expect(list.parentElement).toBe(head.querySelector(".chat-head-text"));
  expect(list.previousElementSibling?.matches('.chat-head-sub, [role="status"]')).toBe(true);

  // Opening takes no focus: the list is not a popover.
  expect(document.activeElement).toBe(button);
  fireEvent.keyDown(button, { key: "Escape" });
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(document.getElementById(list.id)).toBeNull();
  expect(button.hasAttribute("aria-controls")).toBe(false);
  expect(document.activeElement).toBe(button);

  // Pressed again it opens; a second press closes.
  fireEvent.click(button);
  expect(document.getElementById(list.id)).not.toBeNull();
  fireEvent.click(button);
  expect(document.getElementById(list.id)).toBeNull();
});

it("the open list goes with the line and is closed for a new chat", async () => {
  const head = await mountHead(chat("idle"));
  deliver(background());
  fireEvent.click(disclosure()!);
  expect(head.querySelector(".chat-background-list")).not.toBeNull();

  // A turn of work hides the line and its list; back at idle the list is as it was left.
  deliver(chat("working"));
  expect(head.querySelector(".chat-background-list")).toBeNull();
  deliver(chat("idle"));
  expect(head.querySelector(".chat-background-list")).not.toBeNull();

  const next = "00000000-0000-4000-8000-000000000002";
  deliver(chat("idle", { id: next }), background(TASKS, next));
  expect(line(head)).toBe("Waiting on 3 background tasks");
  expect(disclosure()!.getAttribute("aria-expanded")).toBe("false");
  expect(head.querySelector(".chat-background-list")).toBeNull();
});

it("in the sheet Escape on the open disclosure closes the list and leaves the sheet", async () => {
  await mountHead(chat("idle"), true);
  deliver(background());
  const button = disclosure()!;
  button.focus();
  fireEvent.click(button);
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(button.getAttribute("aria-expanded")).toBe("true");

  // The dialog hears Escape on the document before React does: the line takes it first.
  const notPrevented = fireEvent.keyDown(button, { key: "Escape" });
  expect(notPrevented).toBe(false);
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(document.activeElement).toBe(button);

  // With the list shut, Escape is the dialog's again.
  fireEvent.keyDown(button, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
});
