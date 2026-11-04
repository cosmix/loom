import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import draftsFixture from "@/plan/api/fixtures/kickoff-drafts.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { kickoffDraftViewSchema, type KickoffDraftDetail } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { resetKickoffLanding } from "@/plan/new/kickoff-landing";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { kickoffDraftsAtom, planSocketAtom, projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/chat/panel", () => ({
  ChatRegistrations: () => <span>Chat registrations</span>,
  ChatPanel: () => <aside aria-label="Chat">Chat panel</aside>,
}));

const draft = kickoffDraftViewSchema.parse(draftsFixture.drafts[0]);
const ID = draft.session;
const detail: KickoffDraftDetail = {
  draft,
  shape: {
    size: "feature",
    name: "microvm-test-env",
    proposal: false,
    problem: "Tests need isolation.",
    decisions: ["Use a microvm per run."],
    questions: ["Which hypervisor?"],
    sections: [],
  },
  messages: ["I would like microvm support.", "Firecracker, probably."],
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
    for (const handler of this.listeners.get("chat") ?? []) handler({ type: "chat", ...frame });
  }
  result(error: string | null = null) {
    const sent = this.send.mock.calls.at(-1)?.[0] as { client_id: string };
    this.receive({ frame: "result", client_id: sent.client_id, ok: error === null, error });
  }
}

let served: { status: number; body: unknown } = { status: 200, body: detail };

function mount(
  search: string,
  current: ReturnType<typeof liveSession> | null = null,
  drafts: Array<typeof draft> = [draft],
) {
  const store = storeFor();
  const socket = new FakeSocket();
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(kickoffSessionAtom, null);
  store.set(projectAtom, { checklists: [], standalone: [], moves: [] });
  store.set(kickoffDraftsAtom, drafts);
  const router = createMemoryRouter([{ path: "*", element: <KickoffRoute /> }], {
    initialEntries: [{ pathname: "/new", search }],
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, socket, router };
}

function liveSession(id: string) {
  return sessionViewSchema.parse({ ...snapshotFixture.session, id, state: "idle" });
}

beforeEach(() => {
  served = { status: 200, body: detail };
  resetKickoffLanding();
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation((input) => {
      const url = String(input);
      if (!url.startsWith("/api/kickoffs/")) {
        return Promise.resolve(new Response(JSON.stringify(treeFixture)));
      }
      expect(url).toBe(`/api/kickoffs/${ID}`);
      return Promise.resolve(new Response(JSON.stringify(served.body), { status: served.status }));
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("an ended draft opens read-only with what was settled and what the user typed", async () => {
  mount(`?session=${ID}`);
  expect(await screen.findByRole("heading", { name: "microvm-test-env" })).toBeTruthy();
  const messages = within(screen.getByRole("region", { name: "Your messages" }));
  expect(messages.getAllByRole("listitem").map((item) => item.textContent)).toEqual(
    detail.messages,
  );
  const row = (name: string) => within(screen.getByRole("region", { name }));
  expect(row("Problem").getByText("Tests need isolation.")).toBeTruthy();
  expect(row("Settled decisions").getByText("Use a microvm per run.")).toBeTruthy();
  expect(row("Open questions").getByText("Which hypervisor?")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("a draft with no name is Untitled with the date it started", async () => {
  served = { status: 200, body: { ...detail, draft: { ...draft, name: null } } };
  mount(`?session=${ID}`);
  const heading = await screen.findByRole("heading", { level: 1 });
  expect(heading.textContent).toMatch(/^Untitled · /);
});

it("an unknown draft is an error with a way back to New work", async () => {
  served = { status: 404, body: { error: "unknown_draft", message: "No such draft" } };
  mount(`?session=${ID}`);
  expect(await screen.findByText("That draft is gone")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Start new work" }).getAttribute("href")).toBe("/new");
});

it("a draft the server still runs waits for its conversation, no Continue", async () => {
  served = { status: 200, body: { ...detail, draft: { ...draft, live: true } } };
  mount(`?session=${ID}`);
  expect(await screen.findByRole("status", { name: "Opening the conversation" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Continue this draft" })).toBeNull();
});

it("Continue this draft sends continue_kickoff and follows the new conversation", async () => {
  const { store, socket, router } = mount(`?session=${ID}`);
  fireEvent.click(await screen.findByRole("button", { name: "Continue this draft" }));
  expect(socket.send).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ type: "chat", command: "continue_kickoff", session: ID }),
  );
  await act(async () => socket.result());
  // The new kickoff's session arrives; the route moves this tab to its address.
  const next = liveSession("22222222-2222-4222-8222-222222222222");
  act(() => store.set(snapshotAtom, { session: next, turns: [], queue: [], previous: null }));
  await waitFor(() => expect(router.state.location.search).toBe(`?session=${next.id}`));
  expect(await screen.findByRole("log", { name: "Chat transcript" })).toBeTruthy();
  expect(store.get(kickoffSessionAtom)).toBeNull();
});

it("Discard asks first, then sends discard_kickoff and returns to New work", async () => {
  const { socket, router } = mount(`?session=${ID}`);
  fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
  expect(socket.send).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Keep it" }));
  expect(screen.getByRole("button", { name: "Continue this draft" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  fireEvent.click(screen.getByRole("button", { name: "Discard for good" }));
  expect(socket.send).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ command: "discard_kickoff", session: ID }),
  );
  await act(async () => socket.result());
  await waitFor(() => expect(router.state.location.search).toBe(""));
});

it("a refused command shows its reason and keeps the draft", async () => {
  const { socket } = mount(`?session=${ID}`);
  fireEvent.click(await screen.findByRole("button", { name: "Continue this draft" }));
  await act(async () => socket.result("Another chat is open"));
  expect(screen.getByRole("status").textContent).toBe("Another chat is open");
  expect(screen.getByRole("button", { name: "Continue this draft" })).toBeTruthy();
});

it("this tab's own start moves to its address once; /new then stays empty", async () => {
  const session = liveSession(ID);
  const { store, router } = mount("", session);
  store.set(kickoffSessionAtom, ID);
  await waitFor(() => expect(router.state.location.search).toBe(`?session=${ID}`));
  expect(store.get(kickoffSessionAtom)).toBeNull();

  await act(() => router.navigate("/new"));
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
  expect(router.state.location.search).toBe("");
});

it("a running kickoff is offered above the empty form and can be set aside", async () => {
  const running = { ...draft, name: null, live: true };
  const { socket } = mount("", liveSession(ID), [running]);
  const note = screen.getByText((_, element) => element?.textContent === "Untitled is still open");
  expect(note).toBeTruthy();
  expect(screen.getByRole("textbox", { name: /What do you want to build/ })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Set it aside" }));
  expect(socket.send).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ command: "set_aside_kickoff", session: ID }),
  );
  await act(async () => socket.result());
});
