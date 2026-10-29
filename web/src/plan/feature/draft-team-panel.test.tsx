import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { serverInfoSchema, type ClientMessage, type PlanMessage } from "@/plan/api/schema";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { DraftTeamView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import {
  authoringAtom,
  connectionAtom,
  draftTeamAtom,
  planSocketAtom,
  serverInfoAtom,
} from "@/plan/state/atoms";

const server = serverInfoSchema.parse(hello.server);
const session = sessionViewSchema.parse({ ...snapshotFixture.session, id: "draft-session" });
const paths = [
  "doc/product/feature/section_01.md",
  "doc/product/feature/section_02.md",
  "doc/product/feature/section_03.md",
] as const;

function teamView(): DraftTeamView {
  return {
    session: "draft-session",
    documents: [...paths],
    teammates: [
      {
        agent_id: "agent-1",
        name: "Ada",
        documents: [paths[0], paths[1]],
        current: paths[1],
        state: "working",
        tokens: { input: 1000, cache_write: 200, cache_read: 300, output: 400 },
      },
      {
        agent_id: "agent-2",
        name: "Grace",
        documents: [paths[2]],
        current: null,
        state: "working",
        tokens: { input: 2000, cache_write: 100, cache_read: 500, output: 500 },
      },
    ],
  };
}

function storeFor(team: DraftTeamView | null = teamView(), current: SessionView | null = session) {
  const store = createStore();
  store.set(draftTeamAtom, team);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(serverInfoAtom, { ...server, read_only: [] });
  store.set(authoringAtom, { holder: null });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  return store;
}

function renderPanel(store = storeFor(), feature = "feature") {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <DraftTeamPanel feature={feature} />
      </MemoryRouter>
    </Provider>,
  );
}

function installSocket(store: ReturnType<typeof createStore>) {
  const handlers = new Set<(message: PlanMessage) => void>();
  const send = vi.fn((_message: ClientMessage) => true);
  const socket: ReturnType<typeof connectPlanSocket> = {
    send,
    subscribe(type, handler) {
      const dispatch = (message: PlanMessage) => {
        if (message.type === type) handler(message as Parameters<typeof handler>[0]);
      };
      handlers.add(dispatch);
      return () => {
        handlers.delete(dispatch);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket);
  const reply = (error: string | null = null) => {
    const request = send.mock.calls.at(-1)?.[0];
    if (request?.type !== "chat") throw new Error("Expected a chat command");
    const message: PlanMessage = {
      type: "chat",
      frame: "result",
      client_id: request.client_id,
      ok: error === null,
      error,
    };
    [...handlers].forEach((handler) => handler(message));
  };
  return { send, reply };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DraftTeamPanel", () => {
  it("renders nothing without a draft team", () => {
    const { container } = renderPanel(storeFor(null));

    expect(container.innerHTML).toBe("");
  });

  it("hides a feature A team on feature B while its matching session is active", () => {
    renderPanel(storeFor(), "other");

    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });

  it("hides a team on its own feature during another active session", () => {
    renderPanel(storeFor(teamView(), { ...session, id: "vision-session" }));

    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });

  it.each([
    [[paths[0]], true],
    [["doc/plans/feature/PLAN-one.md"], true],
    [[paths[0], "doc/plans/feature/PLAN-one.md"], true],
    [["doc/product/feature-other/section_01.md"], false],
    [["doc/plans/other/PLAN-one.md"], false],
    [["doc/product/other/section_01.md", paths[0]], true],
    [[], true],
  ] as const)("scopes an active team's documents %j to the feature", (documents, visible) => {
    const team = { ...teamView(), documents: [...documents] };
    renderPanel(storeFor(team));

    expect(screen.queryByRole("region", { name: "Draft team" }) !== null).toBe(visible);
    expect(screen.queryByRole("button", { name: "Stop the team" }) !== null).toBe(visible);
  });

  it.each([null, "ended", "failed"] as const)(
    "hides the panel when the matching session is %s",
    (state) => {
      renderPanel(storeFor(teamView(), state === null ? null : { ...session, state }));

      expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
    },
  );

  it("hides a finished team even on its own feature after the session ends", () => {
    const team = teamView();
    team.teammates = team.teammates.map((teammate) => ({ ...teammate, state: "idle" }));
    renderPanel(storeFor(team, { ...session, state: "ended" }));

    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
  });

  it("shows the active matching session even before feature documents arrive", () => {
    renderPanel(storeFor({ ...teamView(), documents: [], teammates: [] }), "other");

    expect(screen.getByRole("region", { name: "Draft team" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop the team" })).toBeTruthy();
  });

  it("shows the ordered documents with their owners and three queue states", () => {
    renderPanel();

    const panel = screen.getByRole("region", { name: "Draft team" });
    expect(within(panel).getByRole("heading", { name: "Drafting with a team" }).tagName).toBe("H2");
    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(queue.tagName).toBe("OL");
    const rows = within(queue).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("Ada")).toBeTruthy();
    expect(within(rows[0]).getByText("Drafted")).toBeTruthy();
    expect(within(rows[1]).getByText("Ada")).toBeTruthy();
    expect(within(rows[1]).getByText("Drafting")).toBeTruthy();
    expect(within(rows[2]).getByText("Grace")).toBeTruthy();
    expect(within(rows[2]).getByText("Queued")).toBeTruthy();
    expect(within(rows[0]).getByRole("link", { name: paths[0] }).getAttribute("href")).toBe(
      "/files/product/feature/section_01.md",
    );
    expect(
      within(queue)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(paths);
    expect(queue.querySelectorAll('[data-kind="drafting"]')).toHaveLength(1);
    expect(rows[1].querySelector('[data-kind="drafting"]')).not.toBeNull();
    expect(queue.querySelectorAll(".lucide-book-open")).toHaveLength(3);
  });

  it("shows all token counts including cached tokens without a tooltip", () => {
    renderPanel();

    expect(screen.getByText("2 teammates, 3 documents · 5.0k tokens total")).toBeTruthy();
    const teammates = screen.getByRole("list", { name: "Teammates" });
    const rows = within(teammates).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("Ada")).toBeTruthy();
    expect(within(rows[0]).getByText("working")).toBeTruthy();
    expect(within(rows[0]).getByText(paths[1])).toBeTruthy();
    expect(within(rows[0]).getByText("1.9k tokens")).toBeTruthy();
    expect(
      within(rows[0]).getByText("1.0k in / 400 out · 200 cache write · 300 cache read"),
    ).toBeTruthy();
    expect(within(rows[1]).getByText("Grace")).toBeTruthy();
    expect(within(rows[1]).getByText("3.1k tokens")).toBeTruthy();
    expect(within(rows[1]).getByText("Current document: None")).toBeTruthy();
    expect(
      within(rows[1]).getByText("2.0k in / 500 out · 100 cache write · 500 cache read"),
    ).toBeTruthy();
    expect(teammates.querySelector("[title]")).toBeNull();
  });

  it.each([
    ["working", null, "Queued"],
    ["working", "doc/product/unknown.md", "Queued"],
    ["idle", null, "Drafted"],
    ["idle", paths[0], "Queued"],
    ["idle", paths[1], "Drafted"],
  ] as const)("shows %s with current %s as %s", (state, current, expected) => {
    const team = teamView();
    team.teammates[0] = { ...team.teammates[0], state, current };
    renderPanel(storeFor(team));

    const queue = screen.getByRole("list", { name: "Document queue" });
    const row = within(queue).getAllByRole("listitem")[0];
    expect(within(row).getByText(expected)).toBeTruthy();
    expect(row.querySelector('[data-kind="drafting"]')).toBeNull();
    const teammates = screen.getByRole("list", { name: "Teammates" });
    expect(within(within(teammates).getAllByRole("listitem")[0]).getByText(state)).toBeTruthy();
  });

  it("uses teammate assignment order while preserving the document queue order", () => {
    const team = teamView();
    team.documents = [paths[1], paths[0], paths[2]];
    renderPanel(storeFor(team));

    const queue = screen.getByRole("list", { name: "Document queue" });
    const rows = within(queue).getAllByRole("listitem");
    expect(within(rows[0]).getByRole("link", { name: paths[1] })).toBeTruthy();
    expect(within(rows[0]).getByText("Drafting")).toBeTruthy();
    expect(within(rows[1]).getByText("Drafted")).toBeTruthy();
  });

  it("keeps unassigned documents queued", () => {
    renderPanel(storeFor({ ...teamView(), teammates: [] }));

    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(within(queue).getAllByText("Unassigned")).toHaveLength(3);
    expect(within(queue).getAllByText("Queued")).toHaveLength(3);
  });

  it("isolates server names and paths and encodes file links", () => {
    const path = "doc/plans/feature/PLAN-team #1?\u202E.md";
    const team = teamView();
    team.documents = [path];
    team.teammates = [
      { ...team.teammates[0], name: "Draft\u202Eteam", documents: [path], current: path },
    ];
    renderPanel(storeFor(team));

    const link = screen.getByRole("link", { name: "doc/plans/feature/PLAN-team #1?⟨U+202E⟩.md" });
    expect(link.getAttribute("href")).toBe("/files/plans/feature/PLAN-team%20%231%3F%E2%80%AE.md");
    expect(screen.getAllByText("Draft⟨U+202E⟩team")).toHaveLength(2);
    expect(screen.getAllByText("doc/plans/feature/PLAN-team #1?⟨U+202E⟩.md")).toHaveLength(2);
    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(queue.querySelector(".lucide-list-checks")).not.toBeNull();
  });

  it("confirms before sending the end command and prevents duplicate stops", async () => {
    const store = storeFor();
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    fireEvent.click(button);
    fireEvent.click(button);

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "Stop the team? Documents already written stay.",
    );
    expect(socket.send).toHaveBeenCalledExactlyOnceWith({
      type: "chat",
      command: "end",
      client_id: expect.any(String),
    });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Stopping the team")).toBeTruthy();
    await act(async () => socket.reply());
    expect(button.getAttribute("aria-disabled")).toBeNull();
    expect(store.get(draftTeamAtom)?.documents).toEqual(paths);
  });

  it("does not send when confirmation is declined", () => {
    const store = storeFor();
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderPanel(store);

    fireEvent.click(screen.getByRole("button", { name: "Stop the team" }));

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "Stop the team? Documents already written stay.",
    );
    expect(socket.send).not.toHaveBeenCalled();
  });

  it("does not end another session that becomes current during confirmation", () => {
    const store = storeFor();
    const socket = installSocket(store);
    vi.spyOn(window, "confirm").mockImplementation(() => {
      store.set(snapshotAtom, {
        ...store.get(snapshotAtom),
        session: { ...session, id: "vision" },
      });
      return true;
    });
    renderPanel(store);

    fireEvent.click(screen.getByRole("button", { name: "Stop the team" }));

    expect(socket.send).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });

  it("shows the read-only reason and a stop button that cannot send", () => {
    const store = storeFor();
    store.set(serverInfoAtom, server);
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm");
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    fireEvent.click(button);

    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Stopping the team requires authoring")).toBeTruthy();
    expect(
      screen.getByText("loom plan is in read-only mode: Claude is not logged in"),
    ).toBeTruthy();
    expect(confirm).not.toHaveBeenCalled();
    expect(socket.send).not.toHaveBeenCalled();
  });

  it("renders the other author name through Untrusted in the blocked reason", () => {
    const store = storeFor();
    store.set(authoringAtom, {
      holder: { ...hello.you, name: "Other\u202Eauthor", away_since: null, frees_at: null },
    });
    renderPanel(store);

    expect(screen.getByText("Other⟨U+202E⟩author holds authoring")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Stop the team" }).getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("shows a refused stop as visible text and allows retrying", async () => {
    const store = storeFor();
    const socket = installSocket(store);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    fireEvent.click(button);
    await act(async () => socket.reply("Stop\u202Erefused"));

    expect(screen.getByRole("alert").textContent).toBe("Stop⟨U+202E⟩refused");
    expect(button.getAttribute("aria-disabled")).toBeNull();
  });
});
