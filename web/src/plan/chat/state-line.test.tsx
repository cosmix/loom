import { act, cleanup, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type Item, type SessionView, type Turn } from "@/plan/api/schema-chat";
import { registerOfferAction } from "@/plan/offers/actions";
import {
  authoringAtom,
  checklistAtom,
  presenceAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { RunHeader } from "./run-header";
import { StateLine } from "./state-line";

const snapshot = chatSnapshotSchema.parse(snapshotFixture);
const base = snapshot.session!;
const now = new Date("2026-10-28T10:00:00Z");
const since = "2026-10-28T09:59:19Z";
const server = { ...serverInfoSchema.parse(helloFixture.server), read_only: [] };
const stops: Array<() => void> = [];

function session(fields: Partial<SessionView> = {}): SessionView {
  return {
    ...base,
    state: "idle",
    state_since: since,
    working_since: since,
    subagents_running: 0,
    last_activity: since,
    ...fields,
  };
}

function turn(items: Item[] = [], fields: Partial<Turn> = {}): Turn {
  return {
    ...snapshot.turns[0],
    id: "current",
    open: true,
    started_at: since,
    items,
    tokens: { input: 1000, cache_write: 200, cache_read: 999999, output: 300 },
    ...fields,
  };
}

function permission(tool: string, id = tool): Extract<Item, { type: "permission" }> {
  return {
    type: "permission",
    id,
    request_id: id,
    tool,
    input: "{}",
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2026-10-28T11:00:00Z",
  };
}

function tool(summary: string): Extract<Item, { type: "tool" }> {
  const sample = snapshot.turns[0].items.find((item) => item.type === "tool");
  if (!sample || sample.type !== "tool") throw new Error("Missing tool fixture");
  return { ...sample, summary };
}

function harness(current: SessionView | null = session(), turns = [turn()]) {
  const store = createStore();
  store.set(serverInfoAtom, server);
  store.set(youAtom, { public_id: "holder", name: "Dana" });
  store.set(authoringAtom, {
    holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(snapshotAtom, { session: current, turns, queue: [], previous: null });
  function setSession(next: SessionView | null) {
    act(() => store.set(snapshotAtom, { ...store.get(snapshotAtom), session: next }));
  }
  function setTurns(next: Turn[]) {
    act(() => store.set(snapshotAtom, { ...store.get(snapshotAtom), turns: next }));
  }
  function show() {
    return render(
      <Provider store={store}>
        <StateLine />
      </Provider>,
    );
  }
  function header(value = current!) {
    return render(
      <Provider store={store}>
        <RunHeader session={value} />
      </Provider>,
    );
  }
  return { store, setSession, setTurns, show, header };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(now);
  // The Starting row loads the session brief; it never answers here.
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  stops.splice(0).forEach((stop) => stop());
  vi.useRealTimers();
});

it("every state row", () => {
  const cases: Array<[SessionView["state"] | null, string]> = [
    [null, "No chat"],
    ["starting", "Starting session"],
    ["idle", "Waiting for you"],
    ["working", "Working · 41s"],
    ["permission", "Needs a decision: Bash"],
    ["ending", "Writing final notes into the documents"],
    ["ended", "Chat ended · 41s ago"],
    ["failed", "The session could not start"],
  ];
  for (const [state, words] of cases) {
    const h = harness(
      state ? session({ state, failure: { kind: "spawn", message: "spawn refused" } }) : null,
      [turn([permission("Bash")])],
    );
    const { container } = h.show();
    expect(container.textContent).toContain(words);
    expect(screen.getAllByRole("status")[0].getAttribute("aria-live")).toBe("polite");
    if (state === "starting") {
      expect(container.textContent).toContain(base.name);
      expect(container.textContent).toContain(base.model);
    }
    if (state === "ending") expect(container.textContent).toContain("up to 10 minutes");
    cleanup();
  }
});

it("none offers the checklist primary offer and New chat", () => {
  const h = harness(null);
  const primary = {
    ...checklistViewSchema.parse(helloFixture.checklist).primary!,
    label: "Write section 05",
  };
  const checklist = {
    current: { kind: "ready" as const },
    steps: [],
    primary,
    secondary: [],
    items: [],
  };
  stops.push(registerOfferAction(primary.kind, vi.fn()));
  h.store.set(checklistAtom, checklist);
  h.show();
  expect(screen.getByRole("button", { name: "Write section 05" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
});

it("working shows elapsed, newest tool, subagents", () => {
  const h = harness(
    session({ state: "working", subagents_running: 2, state_since: now.toISOString() }),
    [turn([tool("Old summary"), tool("Inspect doc/product.md")])],
  );
  const { container } = h.show();
  expect(container.textContent).toContain("Working · 41s · Inspect doc/product.md · 2 subagents");
  expect(container.textContent).not.toContain("Old summary");
  act(() => vi.advanceTimersByTime(1000));
  expect(container.textContent).toContain("Working · 42s");
});

it("working falls back to the open turn start", () => {
  const { container } = harness(
    session({ state: "working", working_since: null, state_since: now.toISOString() }),
  ).show();
  expect(container.textContent).toContain("Working · 41s");
});

it("starting shows the Brief link and no other state does", () => {
  const h = harness(session({ state: "starting" }));
  const { container } = h.show();
  expect(screen.getByRole("button", { name: "Brief" })).toBeTruthy();
  expect(container.textContent).toContain(`Starting session · ${base.name} · ${base.model}`);
  h.setSession(session({ state: "working" }));
  expect(screen.queryByRole("button", { name: "Brief" })).toBeNull();
});

it("two pending permissions name both tools and count 2", () => {
  const h = harness(session({ state: "permission" }), [
    turn([permission("Edit")], { id: "newer", started_at: "2026-10-28T09:59:30Z" }),
    turn([permission("Bash")], { id: "older", started_at: since }),
  ]);
  h.show();
  expect(screen.getAllByRole("status")[0].textContent).toBe("Needs 2 decisions: Bash, Edit");
});

it("failed spawn text", () => {
  const { container } = harness(
    session({ state: "failed", failure: { kind: "spawn", message: "tmux refused" } }),
  ).show();
  expect(container.textContent).toContain("The session could not start");
  expect(screen.getByText("tmux refused")).toBeTruthy();
  expect(
    screen.getByText(
      "The documents keep what the session recorded as it went. A new chat starts fresh.",
    ),
  ).toBeTruthy();
});

it("failed died text with last activity", () => {
  const { container } = harness(
    session({ state: "failed", failure: { kind: "died", message: "Process exited" } }),
  ).show();
  expect(container.textContent).toContain("The session stopped unexpectedly");
  expect(container.textContent).toContain(`Last activity ${since} (41s ago)`);
});

it("read-only mode hazard lists problems, no offers", () => {
  const h = harness(null);
  h.store.set(serverInfoAtom, {
    ...server,
    read_only: [
      { what: "tmux", message: "tmux missing", fix: "Install tmux" },
      { what: "claude_login", message: "Claude logged out", fix: "Log in to Claude" },
    ],
  });
  const { container } = h.show();
  for (const text of [
    "Read-only",
    "tmux missing",
    "Install tmux",
    "Claude logged out",
    "Log in to Claude",
  ]) {
    expect(container.textContent).toContain(text);
  }
  expect(container.querySelector(".hazard-warning")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});

it("announces state changes only, never elapsed time or tool summaries", () => {
  const h = harness(session({ state: "working" }), [turn([tool("First tool")])]);
  const { container } = h.show();
  const [status] = screen.getAllByRole("status");
  expect(status.textContent).toBe("Working");
  expect(container.textContent).toContain("Working · 41s · First tool");
  act(() => vi.advanceTimersByTime(20_000));
  h.setTurns([turn([tool("Second tool")])]);
  expect(container.textContent).toContain("Second tool");
  expect(status.textContent).toBe("Working");
  h.setSession(session({ state: "permission" }));
  expect(status.textContent).toBe("Needs a decision");
  h.setSession(session({ state: "ended" }));
  act(() => vi.advanceTimersByTime(20_000));
  expect(status.textContent).toBe("Chat ended");
});

it("hides only the duplicated state words, so the working detail stays reachable", () => {
  const h = harness(session({ state: "working", subagents_running: 1 }), [
    turn([tool("Inspect doc/product.md")]),
  ]);
  const { container } = h.show();
  const hidden = [...container.querySelectorAll("[aria-hidden='true']")].filter(
    (node) => node.textContent === "Working",
  );
  expect(hidden).toHaveLength(1);
  const exposed = screen.getByText(/41s · Inspect doc\/product\.md · 1 subagent/);
  expect(exposed.closest("[aria-hidden='true']")).toBeNull();
  h.setSession(session({ state: "idle" }));
  const idleHidden = [...container.querySelectorAll("[aria-hidden='true']")].map(
    (node) => node.textContent,
  );
  expect(idleHidden).toContain("Waiting for you");
});

it("session total and time left when quota known", () => {
  const h = harness(session(), [turn(), turn([], { id: "second" })]);
  h.header();
  expect(screen.getByText("· 3.0k tokens")).toBeTruthy();
  // The session wire model has no quota reading; omit time left until it supplies one.
  expect(screen.queryByText(/time left|minutes left|lasts to reset/i)).toBeNull();
  h.setTurns([turn()]);
  expect(screen.getByText("· 1.5k tokens")).toBeTruthy();
});

it("away line and return", () => {
  const h = harness(session({ away_until: "2026-10-28T10:10:00Z" }));
  h.store.set(authoringAtom, {
    holder: {
      public_id: "holder",
      name: "Dana",
      away_since: since,
      frees_at: "2026-10-28T10:10:00Z",
    },
  });
  const { container } = h.header();
  expect(container.textContent).toContain("Dana is away · authoring frees at");
  expect(container.querySelector(".chat-run-header")?.textContent).toMatch(/\d{2}:\d{2}/);
  act(() =>
    h.store.set(authoringAtom, {
      holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
    }),
  );
  expect(container.textContent).toContain("Dana authoring");
  expect(container.textContent).not.toContain("is away");
});

it("holder and watcher count with names", () => {
  const h = harness();
  h.store.set(presenceAtom, {
    entries: [
      { public_id: "holder", name: "Dana", holder: true, away: false, reading: null, tabs: 2 },
      { public_id: "viewer1", name: "Alex", holder: false, away: false, reading: null, tabs: 3 },
      { public_id: "viewer2", name: "Sam", holder: false, away: false, reading: null, tabs: 1 },
    ],
  });
  const { container } = h.header();
  expect(container.textContent).toContain("Dana authoring");
  expect(screen.getByText("2 watching")).toBeTruthy();
  expect(screen.getByTitle("Alex, Sam").getAttribute("aria-label")).toBe("2 watching: Alex, Sam");
  act(() => h.store.set(youAtom, { public_id: "viewer1", name: "Alex" }));
  expect(container.textContent).toContain("Read-only · Dana is authoring");
});

it("session strings expose controls as visible text", () => {
  const h = harness(session({ state: "starting", name: "Plan\u202Ename" }));
  const { container } = h.show();
  expect(container.textContent).toContain("Plan⟨U+202E⟩name");
  h.setSession(
    session({ state: "failed", failure: { kind: "spawn", message: "Failed\u200Bhere" } }),
  );
  expect(container.textContent).toContain("Failed⟨U+200B⟩here");
});
