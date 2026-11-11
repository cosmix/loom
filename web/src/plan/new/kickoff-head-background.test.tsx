import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import backgroundFixture from "@/plan/api/fixtures/background.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { installChatSubscription } from "@/plan/chat/model";
import { KickoffHead } from "@/plan/new/kickoff-head";
import { planSocketAtom } from "@/plan/state/atoms";

type Obj = Record<string, unknown>;

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
function chat(state: string): Obj {
  const base = structuredClone(snapshotFixture);
  const session = { ...base.session, state, holder: helloFixture.you.name, queued: 0 };
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

const text = (element: Element | null): string =>
  (element?.textContent ?? "").replace(/\s+/g, " ").trim();

/** The kickoff head on a store fed by the socket: opens it and delivers hello and `first`. */
async function mountHead(first: Obj): Promise<HTMLElement> {
  const store = createStore();
  initClient(store);
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  store.set(planSocketAtom, handle);
  disposers.push(handle.dispose, installChatSubscription(store));
  const { container } = render(
    <Provider store={store}>
      <KickoffHead />
    </Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => FakeSocket.last?.open());
  deliver(hello(), first);
  return container.querySelector<HTMLElement>(".kickoff-head")!;
}

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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("an idle kickoff with no task keeps its sub-line empty", async () => {
  const head = await mountHead(chat("idle"));
  // kickoff.css hides the sub-line on `:empty`: no node may stand in it.
  expect(head.querySelector(".kickoff-head__sub")!.childNodes.length).toBe(0);
  expect(head.children.length).toBe(2);
  expect(screen.queryByRole("status")).toBeNull();
});

it("an idle kickoff that waits on background tasks shows the line and its list", async () => {
  const head = await mountHead(chat("idle"));
  deliver(backgroundFixture);

  const sub = head.querySelector(".kickoff-head__sub")!;
  expect(text(sub.querySelector(".chat-background-line"))).toBe("Waiting on 3 background tasks");
  expect(sub.querySelector(".chat-state-row")).toBeNull();
  expect(text(screen.getByRole("status"))).toBe("Waiting on 3 background tasks");

  const disclosure = screen.getByRole("button", { name: "Background tasks" });
  fireEvent.click(disclosure);
  const list = document.getElementById(disclosure.getAttribute("aria-controls")!)!;
  // The list stands in the head, after the sub-line that would clip it.
  expect(list.parentElement).toBe(head);
  expect(list.closest(".kickoff-head__sub")).toBeNull();
  expect(list.querySelectorAll("li").length).toBe(5);
  expect(text(list.querySelector("li:last-child"))).toBe("1 earlier agent");
});

it("a working kickoff with tasks shows its state words alone", async () => {
  const head = await mountHead(chat("working"));
  deliver(backgroundFixture);

  expect(text(head.querySelector(".kickoff-head__sub"))).toBe("Working");
  expect(head.querySelector(".chat-background-line")).toBeNull();
  expect(screen.queryByRole("button", { name: "Background tasks" })).toBeNull();
});
