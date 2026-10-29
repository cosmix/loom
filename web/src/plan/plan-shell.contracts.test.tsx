import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import clientReadingFixture from "@/plan/api/fixtures/client-reading.json";
import clientSetNameFixture from "@/plan/api/fixtures/client-set-name.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import presenceFixture from "@/plan/api/fixtures/presence.json";
import refusedFixture from "@/plan/api/fixtures/refused.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import shareFixture from "@/plan/api/fixtures/share.json";
import {
  authoringViewSchema,
  clientMessageSchema,
  planMessageSchema,
  presenceListSchema,
  refusedSchema,
  serverInfoSchema,
  shareLinkSchema,
} from "@/plan/api/schema";
import { connectPlanSocket } from "@/plan/api/socket";
import { Share } from "@/plan/header/share";
import { PlanShell } from "@/plan/shell";
import { unauthorizedAtom } from "@/plan/state/atoms";

class FakeSocket {
  static instances: FakeSocket[] = [];

  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }

  send(): void {}

  close(): void {
    this.readyState = 3;
  }

  open(): void {
    this.readyState = 1;
    this.onopen?.(new Event("open"));
  }

  receive(frame: unknown): void {
    this.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent);
  }

  closeFromServer(): void {
    this.readyState = 3;
    this.onclose?.({ code: 1006, reason: "" } as CloseEvent);
  }
}

type Timer = { id: number; callback: () => void; delay: number; done: boolean };

let timers: Timer[] = [];
let nextTimerId = 1;

function fakeSetTimeout(callback: () => void, delay?: number): number {
  const timer = { id: nextTimerId++, callback, delay: delay ?? 0, done: false };
  timers.push(timer);
  return timer.id;
}

function fakeClearTimeout(id: number): void {
  const timer = timers.find((candidate) => candidate.id === id);
  if (timer) {
    timer.done = true;
  }
}

function pendingTimer(): Timer | undefined {
  return timers.filter((timer) => !timer.done).at(-1);
}

// Fires the newest pending timer, the reconnect the last close or failure scheduled.
function fireReconnect(): number {
  const timer = pendingTimer();
  if (!timer) {
    throw new Error("expected a scheduled reconnect");
  }
  timer.done = true;
  timer.callback();
  return timer.delay;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function requestPath(input: RequestInfo | URL): string {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost").pathname;
}

function openSocket(store: ReturnType<typeof createStore>) {
  return connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
    setTimeout: fakeSetTimeout as unknown as typeof setTimeout,
    clearTimeout: fakeClearTimeout as unknown as typeof clearTimeout,
  });
}

async function waitForSocket(count: number): Promise<FakeSocket> {
  await vi.waitFor(() => expect(FakeSocket.instances).toHaveLength(count));
  return FakeSocket.instances[count - 1]!;
}

let store: ReturnType<typeof createStore>;
let disposers: Array<() => void> = [];

beforeEach(() => {
  FakeSocket.instances = [];
  timers = [];
  nextTimerId = 1;
  disposers = [];
  store = createStore();
  initClient(store);
});

afterEach(() => {
  for (const dispose of disposers) {
    dispose();
  }
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("plan shell contracts", () => {
  it("plan shell shows only the sign-in notice after a 401", () => {
    const fetchMock = vi.fn(async () => jsonResponse({ error: "unauthenticated" }, 401));
    vi.stubGlobal("fetch", fetchMock);
    store.set(unauthorizedAtom, true);
    const { container } = render(
      <Provider store={store}>
        <MemoryRouter initialEntries={["/"]}>
          <Routes>
            <Route path="/" element={<PlanShell />}>
              <Route index element={<p>outlet content</p>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </Provider>,
    );
    const text = (container.textContent ?? "").replace(/\s+/g, " ");
    expect(text).toContain("Open the link loom plan printed.");
    expect(text).toContain("Running loom plan again in this project prints it.");
    expect(text).not.toContain("loom plan restarted");
    expect(screen.queryByText("outlet content")).toBeNull();
    expect(screen.queryByRole("button", { name: /share/i })).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("plan socket reconnects after 1 2 5 and then every 10 seconds", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(serverInfoFixture)),
    );
    const handle = openSocket(store);
    disposers.push(() => handle.dispose());
    const delays: number[] = [];
    for (let attempt = 1; attempt <= 5; attempt += 1) {
      const socket = await waitForSocket(attempt);
      socket.closeFromServer();
      await vi.waitFor(() => expect(pendingTimer()).toBeDefined());
      delays.push(fireReconnect());
    }
    await waitForSocket(6);
    expect(delays).toEqual([1000, 2000, 5000, 10000, 10000]);
  });

  it("plan socket keeps retrying while the server is unreachable", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockImplementation(async () => jsonResponse(serverInfoFixture));
    vi.stubGlobal("fetch", fetchMock);
    const handle = openSocket(store);
    disposers.push(() => handle.dispose());
    const delays: number[] = [];
    for (let step = 0; step < 4 && FakeSocket.instances.length === 0; step += 1) {
      await vi.waitFor(() => {
        if (FakeSocket.instances.length === 0 && !pendingTimer()) {
          throw new Error("neither a retry nor a connect is scheduled");
        }
      });
      if (FakeSocket.instances.length === 0) {
        delays.push(fireReconnect());
      }
    }
    await waitForSocket(1);
    const serverCalls = fetchMock.mock.calls.filter(
      ([input]) => requestPath(input) === "/api/server",
    );
    expect(serverCalls).toHaveLength(3);
    // The two failures schedule the start of the 1 s, 2 s, 5 s sequence; whether
    // the first attempt itself waits is the socket's choice, so either prefix holds.
    expect(delays.length).toBeGreaterThanOrEqual(2);
    expect(delays).toEqual([1000, 2000, 5000].slice(0, delays.length));
  });

  it("plan socket calls resync handlers on the hello after a reconnect", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(serverInfoFixture)),
    );
    const handle = openSocket(store);
    disposers.push(() => handle.dispose());
    const resync = vi.fn();
    disposers.push(handle.onResync(resync));
    const first = await waitForSocket(1);
    first.open();
    first.receive(helloFixture);
    expect(resync).not.toHaveBeenCalled();
    first.closeFromServer();
    await vi.waitFor(() => expect(pendingTimer()).toBeDefined());
    fireReconnect();
    const second = await waitForSocket(2);
    second.open();
    expect(resync).not.toHaveBeenCalled();
    second.receive(helloFixture);
    expect(resync).toHaveBeenCalledTimes(1);
  });

  it("share popover fetches the link on every open", async () => {
    const fetchMock = vi.fn<typeof fetch>(async (input) =>
      requestPath(input) === "/api/share"
        ? jsonResponse(shareFixture)
        : jsonResponse(serverInfoFixture),
    );
    vi.stubGlobal("fetch", fetchMock);
    store.set(unauthorizedAtom, false);
    render(
      <Provider store={store}>
        <TooltipProvider>
          <Share />
        </TooltipProvider>
      </Provider>,
    );
    const shareCalls = () =>
      fetchMock.mock.calls.filter(([input]) => requestPath(input) === "/api/share");
    const trigger = screen.getByRole("button", { name: /^share/i });
    expect(shareCalls()).toHaveLength(0);
    fireEvent.click(trigger);
    expect(await screen.findByText(shareFixture.url)).toBeTruthy();
    expect(shareCalls()).toHaveLength(1);
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.queryByText(shareFixture.url)).toBeNull());
    fireEvent.click(trigger);
    await waitFor(() => expect(shareCalls()).toHaveLength(2));
    expect(await screen.findByText(shareFixture.url)).toBeTruthy();
  });

  it("plan schema parses every wire fixture", () => {
    const cases = [
      { name: "hello", schema: planMessageSchema, fixture: helloFixture },
      { name: "server-info", schema: serverInfoSchema, fixture: serverInfoFixture },
      { name: "presence", schema: presenceListSchema, fixture: presenceFixture },
      { name: "authoring", schema: authoringViewSchema, fixture: authoringFixture },
      { name: "checklist", schema: planMessageSchema, fixture: checklistFixture },
      { name: "share", schema: shareLinkSchema, fixture: shareFixture },
      { name: "refused", schema: refusedSchema, fixture: refusedFixture },
      { name: "client-set-name", schema: clientMessageSchema, fixture: clientSetNameFixture },
      { name: "client-reading", schema: clientMessageSchema, fixture: clientReadingFixture },
    ] as const;
    for (const { name, schema, fixture } of cases) {
      expect(schema.safeParse(fixture).success, `${name} parses`).toBe(true);
      const extra = { ...fixture, unexpected_field: 1 };
      expect(schema.safeParse(extra).success, `${name} rejects an unknown field`).toBe(false);
    }
    const holder = { ...authoringFixture.holder, unexpected_field: 1 };
    expect(authoringViewSchema.safeParse({ holder }).success).toBe(false);
    const entry = { ...presenceFixture.entries[0], unexpected_field: 1 };
    expect(presenceListSchema.safeParse({ entries: [entry] }).success).toBe(false);
  });
});
