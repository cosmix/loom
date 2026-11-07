import { cleanup, render, screen, within } from "@testing-library/react";
import { Provider, type ExtractAtomValue } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { resetKickoffLanding } from "@/plan/new/kickoff-landing";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import * as atoms from "@/plan/state/atoms";

vi.mock("@/plan/chat/panel", () => ({
  ChatRegistrations: () => <span>Chat registrations</span>,
  ChatPanel: () => <aside aria-label="Chat">Chat panel</aside>,
}));

const session = sessionViewSchema.parse({ ...snapshotFixture.session, state: "idle" });
const S = session.id;
const kickoff: KickoffView = {
  session: S,
  target: null,
  shape: {
    size: "feature",
    name: "microvm-test-env",
    proposal: false,
    problem: "Tests need isolation.",
    decisions: [],
    questions: [],
    sections: [],
  },
};
const liveDraft: Drafts[number] = {
  session: S,
  name: "microvm-test-env",
  size: "feature",
  problem: "Tests need isolation.",
  started_at: "2026-11-04T10:47:11Z",
  updated_at: "2026-11-04T10:50:00Z",
  live: true,
};

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

type Drafts = ExtractAtomValue<typeof atoms.kickoffDraftsAtom>;

type Seed = {
  current: SessionView | null;
  view: KickoffView | null;
  drafts: Drafts;
  search?: string;
  state?: { small: unknown } | null;
};

function mount({ current, view, drafts, search = "", state = null }: Seed) {
  const store = storeFor();
  const socket = new FakeSocket();
  store.set(atoms.planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(atoms.kickoffAtom, view);
  store.set(kickoffSessionAtom, null);
  store.set(atoms.projectAtom, { checklists: [], standalone: [], moves: [] });
  store.set(atoms.kickoffDraftsAtom, drafts);
  const router = createMemoryRouter([{ path: "*", element: <KickoffRoute /> }], {
    initialEntries: [{ pathname: "/new", search, state }],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, socket, router };
}

function textOnPage(text: string): HTMLElement[] {
  return screen.queryAllByText(
    (_, element) => element?.textContent?.replace(/\s+/g, " ").includes(text) ?? false,
  );
}

function isChecked(element: HTMLElement): boolean {
  return element.getAttribute("aria-checked") === "true" || (element as HTMLInputElement).checked;
}

beforeEach(() => {
  localStorage.clear();
  resetKickoffLanding();
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(treeFixture)))),
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

it("New work shows an empty form beside an open draft", () => {
  mount({ current: session, view: kickoff, drafts: [liveDraft] });
  expect(screen.getByRole("textbox", { name: /What do you want to build/ })).toBeTruthy();
  expect(textOnPage("microvm-test-env is still open").length).toBeGreaterThan(0);
  const resume = screen.getByRole("link", { name: "Continue" });
  expect(resume.getAttribute("href")).toBe("/new?session=" + S);
});

it("the Name row shows the name and offers no rename", () => {
  mount({
    current: session,
    view: kickoff,
    drafts: [liveDraft],
    search: "?session=" + S,
  });
  const panel = document.querySelector<HTMLElement>('[aria-label="Taking shape"]');
  expect(panel).not.toBeNull();
  const shape = within(panel as HTMLElement);
  expect(shape.getByText(kickoff.shape.name as string)).toBeTruthy();
  expect(shape.queryByRole("button", { name: "Rename" })).toBeNull();
  expect(shape.queryByRole("textbox")).toBeNull();
});

it("Add plan opens New work with Standalone plan chosen", () => {
  mount({ current: null, view: null, drafts: [], state: { small: true } });
  expect(isChecked(screen.getByRole("radio", { name: /Standalone plan/ }))).toBe(true);
  expect(isChecked(screen.getByRole("radio", { name: /Feature/ }))).toBe(false);
});
