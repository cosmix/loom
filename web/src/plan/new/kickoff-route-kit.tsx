import { cleanup, render } from "@testing-library/react";
import { Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { resetKickoffLanding } from "@/plan/new/kickoff-landing";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { kickoffAtom, planSocketAtom, projectAtom } from "@/plan/state/atoms";

export const session = sessionViewSchema.parse({ ...snapshotFixture.session, state: "idle" });
export const LIVE = `?session=${session.id}`;

/** A feature whose vision the landing hook waits for; no metaplan, so no follow-up. */
export const feature: ChecklistView = {
  feature: "planner-search",
  pitch: null,
  has_product: true,
  sections: [],
  plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
  untested: 0,
  metaplans: [],
  plan_rows: [],
  current: { kind: "step", number: 1 },
  steps: [],
  primary: null,
  secondary: [],
  items: [],
};

/** A built feature with a metaplan: what a follow-up can be for. */
export const atlas: ChecklistView = checklistViewSchema.parse({
  ...checklistFixture.checklists[0]!,
  feature: "atlas",
  metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
  plan_rows: [
    {
      slug: "atlas-01",
      path: "doc/plans/atlas/archive/DONE-PLAN-atlas-01.md",
      title: "Plan atlas-01",
      state: "done",
      pressure_date: null,
      listed: true,
    },
  ],
  plans: { listed: 1, total: 1, written: 1, running: 0, done: 1 },
  current: { kind: "ready_all_done" },
  primary: null,
  secondary: [],
  items: [],
  candidates: [],
});

export const kickoff: KickoffView = {
  session: session.id,
  target: null,
  shape: {
    size: "feature",
    name: "planner-search",
    proposal: true,
    problem: "Finding plans takes too long.",
    decisions: [],
    questions: [],
    sections: [],
  },
};

export class FakeSocket {
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

export function mount({
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
  state?: Record<string, unknown> | null;
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

/** The per-test reset: a fetch that serves the tree and refuses drafts, a desktop-wide viewport. */
export function installKickoffRouteLifecycle() {
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
    vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
      matches: query.includes("min-width"),
      media: query,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
    }));
  });
  afterEach(() => {
    cleanup();
    localStorage.clear();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });
}
