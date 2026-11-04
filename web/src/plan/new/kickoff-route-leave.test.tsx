import { act, cleanup, render } from "@testing-library/react";
import { Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import type { ChecklistView } from "@/plan/api/schema";
import type { SessionView } from "@/plan/api/schema-chat";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { resetKickoffLanding } from "@/plan/new/kickoff-landing";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { kickoffAtom, planSocketAtom, projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/chat/panel", () => ({
  ChatRegistrations: () => <span>Chat registrations</span>,
  ChatPanel: () => <aside aria-label="Chat">Chat panel</aside>,
}));

class FakeSocket {
  listeners = new Map<string, Set<(message: unknown) => void>>();
  send = vi.fn((_message: unknown) => true);
  subscribe = (type: string, handler: (message: unknown) => void) => {
    const handlers = this.listeners.get(type) ?? new Set();
    this.listeners.set(type, handlers);
    handlers.add(handler);
    return () => {
      handlers.delete(handler);
    };
  };
  onResync = () => () => {};
  dispose = () => {};
  receive(frame: object) {
    for (const handler of this.listeners.get("chat") ?? []) {
      handler({ type: "chat", ...frame });
    }
  }
  result(error: string | null = null) {
    const sent = this.send.mock.calls.at(-1)?.[0] as { client_id: string };
    this.receive({ frame: "result", client_id: sent.client_id, ok: error === null, error });
  }
}

function mount({
  current = null,
  view = null,
  tab = null,
  state = null,
  search = "",
  checklists = [],
}: {
  current?: SessionView | null;
  view?: KickoffView | null;
  tab?: string | null;
  state?: { small: unknown } | null;
  search?: string;
  checklists?: ChecklistView[];
} = {}) {
  const store = storeFor();
  const socket = new FakeSocket();
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(kickoffAtom, view);
  store.set(kickoffSessionAtom, tab);
  store.set(projectAtom, { checklists, standalone: [], moves: [] });
  const router = createMemoryRouter([{ path: "*", element: <KickoffRoute /> }], {
    initialEntries: [{ pathname: "/new", search, state }],
  });
  const navigate = vi.spyOn(router, "navigate");
  const rendered = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { ...rendered, store, socket, router, navigate };
}

beforeEach(() => {
  localStorage.clear();
  resetKickoffLanding();
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation((input) => {
      const draft = String(input).startsWith("/api/kickoffs/");
      const body = draft ? { error: "not_found" } : treeFixture;
      return Promise.resolve(new Response(JSON.stringify(body), { status: draft ? 404 : 200 }));
    }),
  );
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("leaving /new before the session arrives forgets this tab's start", () => {
  const { store, unmount } = mount();
  act(() => store.set(kickoffSessionAtom, "33333333-3333-4333-8333-333333333333"));
  unmount();
  expect(store.get(kickoffSessionAtom)).toBeNull();
});
