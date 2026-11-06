import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter, RouterProvider, createMemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import {
  activeFeatureAtom,
  authoringAtom,
  connectionAtom,
  planSocketAtom,
  projectAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { ChatPanel } from "./panel";
import { PressureDialog } from "./pressure/pressure-dialog";

type Obj = Record<string, unknown>;

export const TRANSCRIPT_TEXT = "Alpha's transcript line";

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

function helloFrame(): Obj {
  const base = structuredClone(helloFixture);
  const [checklist] = base.checklists;
  return {
    ...base,
    server: { ...base.server, read_only: [] },
    presence: { entries: [] },
    authoring: { holder: { ...base.you, away_since: null, frees_at: null } },
    checklists: [
      { ...checklist, feature: "alpha" },
      { ...checklist, feature: "beta" },
      { ...checklist, feature: "demo" },
    ],
  };
}

/**
 * A snapshot frame for a session waiting on a Bash permission. A session names the feature it was
 * started for in the `feature` field of its session view, on the wire beside `offer`.
 */
export function waitingSessionFrame(feature: string): Obj {
  const base = structuredClone(chatFixture);
  const permission = base.turns[0].items.find((item) => item.type === "permission");
  const items = [
    { type: "text", id: "text-1", markdown: TRANSCRIPT_TEXT },
    {
      ...permission,
      id: "permission-1",
      request_id: "request-1",
      tool: "Bash",
      status: "pending",
      expires_at: "2099-01-01T00:00:00Z",
    },
  ];
  return {
    ...base,
    type: "chat",
    frame: "snapshot",
    session: { ...base.session, state: "permission", feature },
    turns: [{ ...base.turns[0], id: "t1", open: true, changed_files: [], items }],
    queue: [],
    previous: null,
  };
}

/** Renders ChatPanel on the real route shape at desktop width and delivers `frame` by socket. */
export async function mountPanel(path: string, frame: Obj): Promise<HTMLElement> {
  const store = createStore();
  initClient(store);
  store.set(activeFeatureAtom, path.split("/")[2] ?? null);
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  FakeSocket.last = undefined;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  disposers.push(handle.dispose);
  store.set(planSocketAtom, handle);
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <ChatPanel /> }], {
    initialEntries: [path],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => {
    FakeSocket.last?.open();
    FakeSocket.last?.receive(helloFrame());
    FakeSocket.last?.receive(frame);
  });
  return screen.findByRole("complementary", { name: "Chat" });
}

function jsonReply(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

/** Answers both documented title sources: the `/api/doc` view and the standalone plan's title. */
function stubPressureFetch(plan: string, title: string) {
  const options = { ...optionsFixture, plans: [plan] };
  const doc = { ...docFixture, feature: null, path: plan, title, text: `# ${title}\n\nBody.\n` };
  const spy = vi.fn<typeof fetch>(async (input) => {
    const path = new URL(String(input), "http://localhost").pathname;
    if (path === "/api/pressure/options") return jsonReply(options);
    if (path === "/api/pressure/start") return jsonReply(runFixture);
    if (path === "/api/doc") return jsonReply(doc);
    return jsonReply({ error: "not_found", message: "not found" }, 404);
  });
  vi.stubGlobal("fetch", spy);
  return spy;
}

/** Opens PressureDialog as the authoring holder for a written plan and waits for its form. */
export async function mountPressure(plan: string, title: string) {
  const spy = stubPressureFetch(plan, title);
  const base = structuredClone(helloFixture);
  const slug = plan.replace(/^.*PLAN-/, "").replace(/\.md$/, "");
  const store = createStore();
  store.set(serverInfoAtom, { ...base.server, read_only: [] });
  store.set(youAtom, base.you);
  store.set(authoringAtom, { holder: { ...base.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  store.set(projectAtom, {
    checklists: [],
    moves: [],
    standalone: [{ slug, path: plan, state: "written", title, last_change: null }],
  });
  store.set(pressureDialogAtom, { open: true, plan });
  initClient(store);
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureDialog />
      </MemoryRouter>
    </Provider>,
  );
  await waitFor(() =>
    expect(screen.queryByRole("status", { name: "Loading pressure options" })).toBeNull(),
  );
  return { fetchSpy: spy };
}

/** Registers the desktop width stub and the per-test teardown; call once in the contract file. */
export function installRepairLifecycle(): void {
  beforeEach(() => {
    vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
      matches: query.includes("min-width"),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: () => true,
    }));
    // The socket probes /api/server before it opens.
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>(async (input) =>
        new URL(String(input), "http://localhost").pathname === "/api/server"
          ? jsonReply({ ...helloFixture.server, read_only: [] })
          : jsonReply({ error: "not_found", message: "not found" }, 404),
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
}
