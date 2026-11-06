import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { Dock } from "@/plan/dock";
import { treeAtom } from "@/plan/files/atoms";
import {
  atlas,
  feature,
  installKickoffRouteLifecycle,
  kickoff,
  LIVE,
  mount,
  session,
} from "@/plan/new/kickoff-route-kit";
import { authoringAtom, kickoffAtom, projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/chat/panel", () => ({
  ChatRegistrations: () => <span>Chat registrations</span>,
  ChatPanel: () => <aside aria-label="Chat">Chat panel</aside>,
}));

installKickoffRouteLifecycle();

const conversation = () => screen.getByRole("region", { name: "Kickoff conversation" });

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
  fireEvent.click(screen.getByRole("button", { name: "Attach a document" }));

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

it("lands a started follow-up's chat on the feature's metaplan", async () => {
  const { socket, router } = mount({ checklists: [atlas] });
  fireEvent.click(screen.getByRole("radio", { name: "Follow-up" }));
  const picker = screen.getByRole("listbox", { name: "Which feature?" });
  fireEvent.click(within(picker).getByRole("option", { name: /atlas/ }));
  fireEvent.change(screen.getByRole("textbox", { name: "What do you want to build?" }), {
    target: { value: "Tighten the settings dialog" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Start the conversation" }));
  expect(socket.send).toHaveBeenCalledWith(
    expect.objectContaining({ command: "start", offer: "plan-follow-ups", feature: "atlas" }),
  );
  expect(router.state.location.pathname).toBe("/new");

  await act(async () => socket.result());

  await waitFor(() => expect(router.state.location.pathname).toBe("/f/atlas/metaplan"));
});

it("shows the real transcript, composer, Taking shape and an unconfirmed proposal", () => {
  mount({ current: session, view: kickoff, search: LIVE });
  const region = conversation();
  expect(region.classList.contains("kickoff-transcript")).toBe(true);
  const head = region.querySelector<HTMLElement>(".kickoff-head")!;
  expect(within(head).getByRole("heading", { level: 1, name: "planner-search" })).toBeTruthy();
  // Idle: the head's sub-line says nothing and the head draws no spinner.
  expect(head.querySelector(".chat-state-words")).toBeNull();
  expect(head.querySelector(".plan-spinner")).toBeNull();
  expect(screen.queryByText("New work")).toBeNull();
  expect(screen.queryByText("Feature", { selector: ".plan-eyebrow" })).toBeNull();
  expect(within(region).getByRole("log", { name: "Chat transcript" })).toBeTruthy();
  expect(within(region).getByRole("textbox", { name: "Message" })).toBeTruthy();
  const panel = screen.getByRole("complementary", { name: "Taking shape" });
  expect(panel.classList.contains("font-sans")).toBe(true);
  expect(within(panel).getByText("Finding plans takes too long.")).toBeTruthy();
  expect(screen.getByRole("region", { name: "Proposal" })).toBeTruthy();
  expect(screen.getByText(/^Nothing is written yet\./).textContent).toContain(
    "'Write the vision' creates the vision with the open questions in it.",
  );
  const write = screen.getByRole("button", { name: "Write the vision" });
  expect(write.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Settle the size and name first")).toBeTruthy();
  expect(screen.queryByText("Waiting for you")).toBeNull();
});

it("draws the pane's one spinner for a starting session, in the transcript and not the head", () => {
  mount({ current: { ...session, state: "starting" }, search: LIVE });
  const region = conversation();
  expect(region.querySelectorAll(".plan-spinner")).toHaveLength(1);
  const head = region.querySelector<HTMLElement>(".kickoff-head")!;
  expect(head.querySelector(".plan-spinner")).toBeNull();
  expect(head.querySelector(".chat-state-words")?.textContent).toBe("Starting");
  expect(within(head).getByRole("heading", { level: 1 }).textContent).toBe("New work");
});

it("draws no spinner for an idle session", () => {
  mount({ current: session, search: LIVE });
  expect(conversation().querySelectorAll(".plan-spinner")).toHaveLength(0);
});

it.each([
  ["kickoff", "New feature"],
  ["kickoff-small", "New standalone plan"],
])("titles an unnamed kickoff started as %s by its kind", (offer, title) => {
  mount({ current: { ...session, offer }, search: LIVE });
  expect(within(conversation()).getByRole("heading", { level: 1 }).textContent).toBe(title);
});

it("asks the session to change the size from the segmented control", async () => {
  const { socket } = mount({ current: session, view: kickoff, search: LIVE });
  const feature = screen.getByRole("button", { name: "Feature" });
  const plan = screen.getByRole("button", { name: "Standalone plan" });
  expect(feature.getAttribute("aria-pressed")).toBe("true");
  expect(plan.getAttribute("aria-pressed")).toBe("false");

  fireEvent.click(feature);
  expect(socket.send).not.toHaveBeenCalled();
  fireEvent.click(plan);
  expect(socket.send).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ command: "send", text: "Make this a standalone plan instead." }),
  );
  await act(async () => socket.result());
});

it("recognizes the addressed kickoff before its shape arrives and consumes live snapshots", () => {
  const { socket } = mount({ current: session, search: LIVE });
  expect(screen.getByRole("log", { name: "Chat transcript" })).toBeTruthy();
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("New work");
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
  // The ended kickoff is a draft now: its conversation is gone and its draft is being opened.
  expect(screen.queryByRole("log")).toBeNull();
  expect(screen.getByRole("status", { name: "Opening the draft" })).toBeTruthy();
});

it.each([null, "doc/product/planner-search/index.md"])(
  "hides the previous kickoff view while the next kickoff waits for its first frame: %s",
  (target) => {
    const next = { ...session, id: "next-session" };
    const { store, router, navigate } = mount({
      current: next,
      search: "?session=next-session",
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
  const first = mount({ current: session, view, search: LIVE });
  expect(first.navigate).not.toHaveBeenCalled();
  act(() => first.store.set(projectAtom, { checklists: [feature], standalone: [], moves: [] }));
  await waitFor(() => expect(first.router.state.location.pathname).toBe("/f/planner-search"));
  expect(first.navigate).toHaveBeenCalledOnce();
  expect(first.navigate.mock.calls[0]?.[1]).toMatchObject({ replace: true });
  first.unmount();

  const remounted = mount({
    current: session,
    view,
    search: LIVE,
    checklists: [feature],
  });
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
  const { store, socket } = mount({ current: session, view: kickoff, search: LIVE });
  act(() => store.set(kickoffAtom, { ...kickoff, target, shape: { ...kickoff.shape, size } }));
  expect(screen.queryByRole("region", { name: "Proposal" })).toBeNull();
  expect(screen.getByText(target)).toBeTruthy();
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
      search: LIVE,
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
    search: LIVE,
  });
  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "other", name: "Other‮author", away_since: null, frees_at: null },
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
    search: LIVE,
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
