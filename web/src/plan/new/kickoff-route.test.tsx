import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import type { ChecklistView } from "@/plan/api/schema";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { Dock } from "@/plan/dock";
import { treeAtom } from "@/plan/files/atoms";
import { resetKickoffLanding } from "@/plan/new/kickoff-landing";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { authoringAtom, kickoffAtom, planSocketAtom, projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/chat/panel", () => ({
  ChatRegistrations: () => <span>Chat registrations</span>,
  ChatPanel: () => <aside aria-label="Chat">Chat panel</aside>,
}));

const session = sessionViewSchema.parse({ ...snapshotFixture.session, state: "idle" });
const feature: ChecklistView = {
  feature: "planner-search",
  current: { kind: "step", number: 1 },
  steps: [],
  primary: null,
  secondary: [],
  items: [],
};
const kickoff: KickoffView = {
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
  checklists = [],
}: {
  current?: SessionView | null;
  view?: KickoffView | null;
  tab?: string | null;
  state?: { small: unknown } | null;
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
    initialEntries: [{ pathname: "/new", state }],
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
    vi
      .fn<typeof fetch>()
      .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(treeFixture)))),
  );
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("shows the empty kickoff when there is no session", () => {
  mount();
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Kickoff conversation" })).toBeNull();
  expect(screen.queryByRole("complementary", { name: "Taking shape" })).toBeNull();
});

it("loads the file tree for the document picker on a fresh visit to /new", async () => {
  const { store } = mount();
  expect(fetch).toHaveBeenCalledWith(
    "/api/tree",
    expect.objectContaining({ method: "GET", credentials: "same-origin" }),
  );
  await waitFor(() => expect(store.get(treeAtom)).toEqual(treeFixture));
  fireEvent.click(screen.getByRole("button", { name: "Start from a document" }));

  const documents = within(screen.getByRole("list", { name: "Documents" }));
  expect(documents.getByRole("button", { name: "Vision" })).toBeTruthy();
  expect(documents.getByRole("button", { name: "File mode" })).toBeTruthy();
  expect(screen.queryByText("No documents available.")).toBeNull();
});

it.each([
  [true, "kickoff-small"],
  [false, "kickoff"],
  ["true", "kickoff"],
])("passes the small location flag %j to the empty kickoff", async (small, offer) => {
  const { socket } = mount({ state: { small } });
  fireEvent.change(screen.getByRole("textbox", { name: "What do you want to build?" }), {
    target: { value: "Fix search" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Start the conversation" }));
  expect(socket.send).toHaveBeenCalledWith(expect.objectContaining({ command: "start", offer }));
  await act(async () => {
    socket.receive({ frame: "snapshot", session, turns: [], queue: [], previous: null });
    socket.result();
  });
});

it("shows the real transcript, composer, Taking shape and an unconfirmed proposal", () => {
  mount({ current: session, view: kickoff });
  const conversation = screen.getByRole("region", { name: "Kickoff conversation" });
  expect(conversation.classList.contains("kickoff-transcript")).toBe(true);
  expect(within(conversation).getByRole("log", { name: "Chat transcript" })).toBeTruthy();
  expect(within(conversation).getByRole("textbox", { name: "Message" })).toBeTruthy();
  const panel = screen.getByRole("complementary", { name: "Taking shape" });
  expect(panel.classList.contains("font-sans")).toBe(true);
  expect(within(panel).getByText("Finding plans takes too long.")).toBeTruthy();
  expect(screen.getByRole("region", { name: "Proposal" })).toBeTruthy();
  expect(screen.getByText("Nothing is written under doc/ until you say so.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Write the vision" })).toBeNull();
});

it("recognizes the tab's kickoff before its shape arrives and consumes live snapshots", () => {
  const { socket } = mount({ current: session, tab: session.id });
  expect(screen.getByRole("log", { name: "Chat transcript" })).toBeTruthy();
  expect(
    screen.getByText("Nothing yet. The shape appears here as the conversation settles it."),
  ).toBeTruthy();
  act(() =>
    socket.receive({
      frame: "snapshot",
      session: { ...session, state: "ended" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
});

it.each([null, "doc/product/planner-search/index.md"])(
  "hides the previous kickoff view while the next kickoff waits for its first frame: %s",
  (target) => {
    const next = { ...session, id: "next-session" };
    const { store, router, navigate } = mount({
      current: next,
      tab: next.id,
      view: { ...kickoff, target },
      checklists: [feature],
    });
    const panel = screen.getByRole("complementary", { name: "Taking shape" });
    expect(
      within(panel).getByText(
        "Nothing yet. The shape appears here as the conversation settles it.",
      ),
    ).toBeTruthy();
    expect(screen.queryByText("Finding plans takes too long.")).toBeNull();
    expect(screen.queryByText("planner-search")).toBeNull();
    expect(screen.queryByText("doc/product/planner-search/index.md")).toBeNull();
    expect(screen.queryByRole("region", { name: "Proposal" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Write the vision" })).toBeNull();
    expect(router.state.location.pathname).toBe("/new");
    expect(navigate).not.toHaveBeenCalled();

    act(() => store.set(kickoffAtom, { ...kickoff, session: next.id }));
    expect(screen.getByRole("region", { name: "Proposal" })).toBeTruthy();
  },
);

it.each([
  null,
  { ...session, state: "ended" as const },
  { ...session, state: "failed" as const },
  { ...session, id: "other-session" },
])("ignores a stale confirmed kickoff for an inactive or other session: %j", (current) => {
  const { router, navigate } = mount({
    current,
    tab: session.id,
    view: { ...kickoff, target: "doc/product/planner-search/index.md" },
    checklists: [feature],
  });
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/new");
  expect(navigate).not.toHaveBeenCalled();
});

it("lands an active kickoff when its feature appears and dedupes remounts", async () => {
  const view = { ...kickoff, target: "doc/product/planner-search/index.md" };
  const first = mount({ current: session, view });
  expect(first.navigate).not.toHaveBeenCalled();
  act(() => first.store.set(projectAtom, { checklists: [feature], standalone: [], moves: [] }));
  await waitFor(() => expect(first.router.state.location.pathname).toBe("/f/planner-search"));
  expect(first.navigate).toHaveBeenCalledOnce();
  expect(first.navigate.mock.calls[0]?.[1]).toMatchObject({ replace: true });
  first.unmount();

  const remounted = mount({ current: session, view, checklists: [feature] });
  expect(remounted.router.state.location.pathname).toBe("/new");
  expect(remounted.navigate).not.toHaveBeenCalled();
});

it.each(["ended", "failed"] as const)("shows the empty kickoff for a %s session", (state) => {
  mount({ current: { ...session, state }, view: kickoff });
  expect(screen.queryByRole("log")).toBeNull();
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
});

it("shows the empty kickoff for an unrelated active chat", () => {
  mount({ current: { ...session, id: "another-session" }, view: kickoff, tab: session.id });
  expect(screen.queryByRole("log")).toBeNull();
  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
});

it.each(["feature", "plan"] as const)("writes the confirmed %s through chat", async (size) => {
  const target =
    size === "feature" ? "doc/product/planner-search/index.md" : "doc/plans/PLAN-planner-search.md";
  const label = size === "feature" ? "Write the vision" : "Write the plan";
  const { store, socket } = mount({ current: session, view: kickoff });
  act(() => store.set(kickoffAtom, { ...kickoff, target, shape: { ...kickoff.shape, size } }));
  expect(screen.queryByRole("region", { name: "Proposal" })).toBeNull();
  expect(screen.queryByText("Nothing is written under doc/ until you say so.")).toBeNull();
  const write = screen.getByRole("button", { name: label });
  fireEvent.click(write);
  expect(socket.send).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ type: "chat", command: "send", text: `${label}.`, attachments: [] }),
  );
  expect(write.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(write);
  expect(socket.send).toHaveBeenCalledOnce();
  await act(async () => socket.result());
  expect(write.hasAttribute("aria-disabled")).toBe(false);
});

it.each(["starting", "working", "permission", "ending"] as const)(
  "blocks writing while the session is %s",
  (state) => {
    const { socket } = mount({
      current: { ...session, state },
      view: { ...kickoff, target: "doc/product/planner-search/index.md" },
    });
    const write = screen.getByRole("button", { name: "Write the vision" });
    expect(write.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Wait for the current turn to finish")).toBeTruthy();
    fireEvent.click(write);
    expect(socket.send).not.toHaveBeenCalled();
  },
);

it("keeps writing authoring-gated and isolates the holder's name", () => {
  const { store, socket } = mount({
    current: session,
    view: { ...kickoff, target: "doc/product/planner-search/index.md" },
  });
  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "other", name: "Other\u202eauthor", away_since: null, frees_at: null },
    }),
  );
  const write = screen.getByRole("button", { name: "Write the vision" });
  expect(write.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Writing requires authoring")).toBeTruthy();
  expect(screen.getByText("Other⟨U+202E⟩author holds authoring")).toBeTruthy();
  fireEvent.click(write);
  expect(socket.send).not.toHaveBeenCalled();
});

it("shows a refused write and lets the user retry", async () => {
  const { socket } = mount({
    current: session,
    view: { ...kickoff, target: "doc/product/planner-search/index.md" },
  });
  const write = screen.getByRole("button", { name: "Write the vision" });
  fireEvent.click(write);
  await act(async () => socket.result("Writing was refused"));
  expect(screen.getByRole("status").textContent).toBe("Writing was refused");
  expect(write.hasAttribute("aria-disabled")).toBe(false);
  fireEvent.click(write);
  expect(socket.send).toHaveBeenCalledTimes(2);
  await act(async () => socket.result());
});

it("keeps dock registrations mounted while hiding its panel only on /new", async () => {
  const router = createMemoryRouter([{ path: "*", element: <Dock /> }], {
    initialEntries: ["/f/planner-search"],
  });
  render(<RouterProvider router={router} />);
  const registrations = screen.getByText("Chat registrations");
  expect(screen.getByRole("complementary", { name: "Chat" })).toBeTruthy();
  await act(async () => {
    await router.navigate("/new");
  });
  expect(screen.queryByRole("complementary", { name: "Chat" })).toBeNull();
  expect(screen.getByText("Chat registrations")).toBe(registrations);
  await act(async () => {
    await router.navigate("/files/plans/PLAN-search.md");
  });
  expect(screen.getByRole("complementary", { name: "Chat" })).toBeTruthy();
  expect(screen.getByText("Chat registrations")).toBe(registrations);
});
